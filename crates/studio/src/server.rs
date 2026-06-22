//! HTTP API for the operator UI + the run worker.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::nodes::Services;
use crate::workflow::{
    execute_workflow_observed, NodeDef, Registry, RunContext, RunObserver, WorkflowDef,
};

/// Abort handles for in-flight run tasks, keyed by run id, so a run can be
/// cancelled mid-execution.
pub type RunningMap = Arc<std::sync::Mutex<HashMap<String, tokio::task::AbortHandle>>>;

#[derive(Clone)]
pub struct AppState {
    pub services: Arc<Services>,
    pub registry: Arc<Registry>,
    pub running: RunningMap,
}

/// Spawn a run task, tracking its abort handle so it can be cancelled, and
/// untracking it on completion. Returns the JoinHandle so callers can await it.
pub(crate) fn spawn_tracked(
    running: RunningMap,
    run_id: String,
    fut: impl std::future::Future<Output = ()> + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    let cleanup_map = running.clone();
    let cleanup_id = run_id.clone();
    let task = tokio::spawn(async move {
        fut.await;
        cleanup_map.lock().unwrap().remove(&cleanup_id);
    });
    running.lock().unwrap().insert(run_id, task.abort_handle());
    task
}

pub fn app(state: AppState) -> Router {
    // The webnovel→audiobook node-graph engine (nodes/workflow/ruin/youtube) is
    // kept in the crate and the run worker still runs, but its HTTP surface
    // (novels / selections / nodes / workflows / runs) is intentionally not
    // exposed — the app ships only TTS + dubbing + settings.
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/file", get(serve_file))
        .merge(crate::dub::api::routes())
        .merge(crate::clones::api::routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Config ────────────────────────────────────────────────────────────────────
async fn get_config(State(st): State<AppState>) -> Json<crate::config::AppConfig> {
    Json(st.services.config.read().await.clone())
}
async fn put_config(
    State(st): State<AppState>,
    Json(cfg): Json<crate::config::AppConfig>,
) -> Result<StatusCode, AppError> {
    let s = serde_json::to_string(&cfg).map_err(AppError::internal)?;
    st.services.db.save_config_json(&s).await?;
    *st.services.config.write().await = cfg;
    Ok(StatusCode::NO_CONTENT)
}

// ── Run engine (no HTTP surface; driven by the background worker) ─────────────

fn source_config(graph: &WorkflowDef) -> (Option<String>, u32, u32) {
    for n in &graph.nodes {
        if n.node_type == "Source" {
            let slug = n
                .config
                .get("slug")
                .and_then(|v| v.as_str())
                .map(String::from);
            let first = n.config.get("first").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            // Bounded default (matches SourceHandler): never the whole novel.
            let last = n
                .config
                .get("last")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32)
                .unwrap_or_else(|| first.saturating_add(9));
            return (slug, first, last);
        }
    }
    (None, 1, 10)
}

/// Observer that persists per-node start/finish + I/O for live progress.
struct DbObserver {
    db: crate::db::Db,
    run_id: String,
}
#[async_trait]
impl RunObserver for DbObserver {
    async fn on_start(&self, step_id: &str, node: &NodeDef, seq: usize, input: &Value) {
        let _ = self
            .db
            .step_start(
                &self.run_id,
                step_id,
                &node.node_type,
                seq as i64,
                &input.to_string(),
            )
            .await;
    }
    async fn on_finish(
        &self,
        step_id: &str,
        _node: &NodeDef,
        output: &Value,
        ctx_state: &Value,
        error: Option<&str>,
    ) {
        let status = if error.is_some() { "failed" } else { "done" };
        let out = match error {
            Some(e) => {
                json!({ "error": e, "logs": output.get("logs"), "state": output.get("state") })
            }
            None => output.clone(),
        };
        let _ = self
            .db
            .step_finish(
                &self.run_id,
                step_id,
                status,
                &out.to_string(),
                &ctx_state.to_string(),
            )
            .await;
    }
}

async fn execute_run(
    services: Arc<Services>,
    registry: Arc<Registry>,
    run_id: String,
    graph: WorkflowDef,
    preview: bool,
) {
    let db = services.db.clone();
    let _ = db.set_run_status(&run_id, "running", None).await;
    let mut ctx = RunContext::default();
    if preview {
        let (_slug, first, _last) = source_config(&graph);
        ctx.set("first", json!(first));
        ctx.set("last", json!(first)); // one chapter for a quick preview
    }
    let obs = DbObserver {
        db: db.clone(),
        run_id: run_id.clone(),
    };
    match execute_workflow_observed(&graph, &registry, &mut ctx, &obs).await {
        Ok(_) => {
            let _ = db.set_run_status(&run_id, "done", None).await;
        }
        Err(e) => {
            tracing::error!("run {run_id} failed: {e:#}");
            let _ = db
                .set_run_status(&run_id, "failed", Some(&format!("{e:#}")))
                .await;
        }
    }
}

/// Background worker: claim queued runs and execute them, recording progress.
pub async fn run_worker(services: Arc<Services>, registry: Arc<Registry>, running: RunningMap) {
    loop {
        match services.db.claim_next_run().await {
            Ok(Some((id, graph_json, preview))) => {
                match serde_json::from_str::<WorkflowDef>(&graph_json) {
                    Ok(graph) => {
                        // Spawn (tracked, so it's cancellable) and wait for it,
                        // keeping the worker sequential. Abort → JoinError, ignored.
                        let h = spawn_tracked(
                            running.clone(),
                            id.clone(),
                            execute_run(
                                services.clone(),
                                registry.clone(),
                                id.clone(),
                                graph,
                                preview,
                            ),
                        );
                        let _ = h.await;
                    }
                    Err(e) => {
                        let _ = services
                            .db
                            .set_run_status(&id, "failed", Some(&e.to_string()))
                            .await;
                    }
                }
            }
            Ok(None) => tokio::time::sleep(Duration::from_secs(2)).await,
            Err(e) => {
                tracing::error!("claim run error: {e:#}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

// ── File serving (play/inspect generated media) ───────────────────────────────
#[derive(Deserialize)]
struct FileQuery {
    path: String,
}
async fn serve_file(
    State(st): State<AppState>,
    Query(q): Query<FileQuery>,
) -> Result<Response, AppError> {
    let canon =
        std::fs::canonicalize(&q.path).map_err(|_| AppError::not_found("file not found"))?;
    // Restrict to the work dir.
    let work = std::fs::canonicalize(&st.services.work_dir)
        .unwrap_or_else(|_| st.services.work_dir.clone());
    if !canon.starts_with(&work) {
        return Err(AppError::forbidden("path outside work dir"));
    }
    let ct = match canon.extension().and_then(|e| e.to_str()) {
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        Some("m4a") => "audio/mp4",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    };
    let bytes = tokio::fs::read(&canon).await.map_err(AppError::internal)?;
    Ok(([(header::CONTENT_TYPE, ct)], Bytes::from(bytes)).into_response())
}

// ── error helper ──────────────────────────────────────────────────────────────
pub struct AppError {
    status: StatusCode,
    msg: String,
}
impl AppError {
    pub(crate) fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            msg: e.to_string(),
        }
    }
    pub(crate) fn bad_request(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            msg: e.to_string(),
        }
    }
    pub(crate) fn not_found(m: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            msg: m.into(),
        }
    }
    fn forbidden(m: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            msg: m.into(),
        }
    }
}
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            msg: format!("{e:#}"),
        }
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("request error: {}", self.msg);
        }
        (self.status, Json(json!({ "error": self.msg }))).into_response()
    }
}
