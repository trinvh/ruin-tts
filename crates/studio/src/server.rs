//! HTTP API for the operator UI + the run worker.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{Path as AxPath, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::nodes::{node_specs, Services};
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
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/novels", get(novels))
        .route("/api/selections", get(list_selections).post(add_selection))
        .route("/api/config", get(get_config).put(put_config))
        .route("/api/nodes", get(get_nodes))
        .route("/api/workflows", get(list_workflows).post(save_workflow))
        .route(
            "/api/workflows/{id}",
            get(get_workflow).delete(delete_workflow),
        )
        .route("/api/workflow/default", get(get_default_workflow))
        .route("/api/workflow/loop", get(get_loop_workflow))
        .route(
            "/api/runs",
            get(list_runs).post(create_run).delete(clear_runs),
        )
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/retry", post(retry_run))
        .route("/api/runs/{id}/cancel", post(cancel_run))
        .route("/api/file", get(serve_file))
        .merge(crate::dub::api::routes())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ── Novels / selections / config ──────────────────────────────────────────────
#[derive(Deserialize)]
struct NovelQuery {
    search: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
}

async fn novels(
    State(st): State<AppState>,
    Query(q): Query<NovelQuery>,
) -> Result<Json<Value>, AppError> {
    let page = st
        .services
        .ruin()
        .await
        .list_novels(
            q.search.as_deref(),
            q.page.unwrap_or(1),
            q.limit.unwrap_or(20),
        )
        .await?;
    Ok(Json(json!({ "items": page.items, "meta": {
        "page": page.meta.page, "limit": page.meta.limit, "total": page.meta.total, "totalPages": page.meta.total_pages
    } })))
}

#[derive(Serialize)]
struct SelectionDto {
    slug: String,
    title: String,
    cursor: i64,
    enabled: bool,
}
async fn list_selections(State(st): State<AppState>) -> Result<Json<Vec<SelectionDto>>, AppError> {
    let sels = st.services.db.selections().await?;
    Ok(Json(
        sels.into_iter()
            .map(|s| SelectionDto {
                slug: s.slug,
                title: s.title,
                cursor: s.cursor,
                enabled: s.enabled,
            })
            .collect(),
    ))
}
#[derive(Deserialize)]
struct AddSelection {
    slug: String,
    title: String,
}
async fn add_selection(
    State(st): State<AppState>,
    Json(b): Json<AddSelection>,
) -> Result<StatusCode, AppError> {
    st.services.db.upsert_selection(&b.slug, &b.title).await?;
    Ok(StatusCode::NO_CONTENT)
}

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

// ── Nodes / workflows ─────────────────────────────────────────────────────────
async fn get_nodes() -> Json<Value> {
    Json(node_specs())
}
async fn get_default_workflow() -> Json<WorkflowDef> {
    Json(crate::nodes::default_workflow())
}
async fn get_loop_workflow() -> Json<WorkflowDef> {
    Json(crate::nodes::loop_workflow())
}
async fn list_workflows(State(st): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    let rows = st.services.db.workflows().await?;
    Ok(Json(
        rows.into_iter()
            .filter_map(|(_, g)| serde_json::from_str::<Value>(&g).ok())
            .collect(),
    ))
}
async fn save_workflow(
    State(st): State<AppState>,
    Json(graph): Json<WorkflowDef>,
) -> Result<StatusCode, AppError> {
    let s = serde_json::to_string(&graph).map_err(AppError::internal)?;
    st.services
        .db
        .save_workflow(&graph.id, &graph.name, graph.version as i64, &s)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn get_workflow(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    let g = st
        .services
        .db
        .get_workflow(&id)
        .await?
        .ok_or_else(|| AppError::not_found("workflow not found"))?;
    Ok(Json(serde_json::from_str(&g).map_err(AppError::internal)?))
}
async fn delete_workflow(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<StatusCode, AppError> {
    st.services.db.delete_workflow(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Runs ──────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct RunBody {
    graph: WorkflowDef,
    #[serde(default)]
    preview: bool,
}

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

async fn create_run(
    State(st): State<AppState>,
    Json(body): Json<RunBody>,
) -> Result<Json<Value>, AppError> {
    let (slug, first, last) = source_config(&body.graph);
    let last_disp = if last == u32::MAX {
        "cuối".to_string()
    } else {
        last.to_string()
    };
    let label = format!(
        "{} · {}–{}{}",
        slug.unwrap_or_else(|| "?".into()),
        first,
        last_disp,
        if body.preview { " (xem trước)" } else { "" }
    );
    let id = uuid::Uuid::new_v4().to_string();
    let graph_json = serde_json::to_string(&body.graph).map_err(AppError::internal)?;
    let status = if body.preview { "running" } else { "queued" };
    st.services
        .db
        .create_run(&id, &graph_json, body.preview, &label, status)
        .await?;
    if body.preview {
        spawn_tracked(
            st.running.clone(),
            id.clone(),
            execute_run(
                st.services.clone(),
                st.registry.clone(),
                id.clone(),
                body.graph,
                true,
            ),
        );
    }
    Ok(Json(json!({ "run_id": id })))
}

async fn list_runs(State(st): State<AppState>) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(st.services.db.list_runs(50).await?))
}
async fn get_run(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    st.services
        .db
        .get_run(&id)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::not_found("run not found"))
}

#[derive(Deserialize)]
struct RetryQuery {
    from: String,
}

/// Retry a single node (and its descendants): validate upstream is done,
/// restore the context captured before that node, and re-run from there.
async fn retry_run(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Query(q): Query<RetryQuery>,
) -> Result<Json<Value>, AppError> {
    // A `#` in the step id means a loop iteration → dedicated retry path.
    if q.from.contains('#') {
        return retry_iteration(st, id, q.from).await;
    }
    let graph_json = st
        .services
        .db
        .run_graph(&id)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    let graph: WorkflowDef = serde_json::from_str(&graph_json).map_err(AppError::internal)?;

    let states = st.services.db.step_states(&id).await?;
    let status_by: HashMap<&str, &str> = states
        .iter()
        .map(|(n, s, _)| (n.as_str(), s.as_str()))
        .collect();

    // Input validation: every predecessor of `from` must be done.
    let preds: Vec<&str> = graph
        .edges
        .iter()
        .filter(|e| e.to == q.from)
        .map(|e| e.from.as_str())
        .collect();
    for p in &preds {
        if status_by.get(p).copied() != Some("done") {
            return Err(AppError::bad_request(format!(
                "khối trước '{p}' chưa hoàn thành — không thể chạy lại"
            )));
        }
    }

    // Restore the context from the predecessor that finished last.
    let mut restore_node: Option<&str> = None;
    let mut best_fin = String::new();
    for (n, s, f) in &states {
        if preds.contains(&n.as_str()) && s == "done" {
            if let Some(fin) = f {
                if restore_node.is_none() || fin > &best_fin {
                    best_fin = fin.clone();
                    restore_node = Some(n.as_str());
                }
            }
        }
    }
    let restore = match restore_node {
        Some(n) => st.services.db.step_ctx(&id, n).await?,
        None => None,
    };

    // Subgraph = `from` + all descendants.
    let mut keep = std::collections::HashSet::new();
    keep.insert(q.from.clone());
    let mut stack = vec![q.from.clone()];
    while let Some(cur) = stack.pop() {
        for e in &graph.edges {
            if e.from == cur && keep.insert(e.to.clone()) {
                stack.push(e.to.clone());
            }
        }
    }
    let sub = WorkflowDef {
        id: graph.id.clone(),
        name: graph.name.clone(),
        version: graph.version,
        nodes: graph
            .nodes
            .iter()
            .filter(|n| keep.contains(&n.id))
            .cloned()
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter(|e| keep.contains(&e.from) && keep.contains(&e.to))
            .cloned()
            .collect(),
    };
    let ids: Vec<String> = keep.into_iter().collect();
    st.services.db.reset_steps(&id, &ids).await?;
    st.services.db.set_run_status(&id, "running", None).await?;

    let services = st.services.clone();
    let registry = st.registry.clone();
    let rid = id.clone();
    spawn_tracked(st.running.clone(), id.clone(), async move {
        let mut ctx = match &restore {
            Some(j) => {
                RunContext::from_json(&serde_json::from_str::<Value>(j).unwrap_or(Value::Null))
            }
            None => RunContext::default(),
        };
        let obs = DbObserver {
            db: services.db.clone(),
            run_id: rid.clone(),
        };
        match execute_workflow_observed(&sub, &registry, &mut ctx, &obs).await {
            Ok(_) => {
                let _ = services.db.set_run_status(&rid, "done", None).await;
            }
            Err(e) => {
                let _ = services
                    .db
                    .set_run_status(&rid, "failed", Some(&format!("{e:#}")))
                    .await;
            }
        }
    });
    Ok(Json(json!({ "run_id": id })))
}

// ── Loop-iteration retry helpers ──────────────────────────────────────────────

/// Body node set of `loop_id`: nodes reachable from its "body" handle following
/// default edges, stopping at the loop itself.
fn loop_body_set(graph: &WorkflowDef, loop_id: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let mut stack: Vec<String> = graph
        .edges
        .iter()
        .filter(|e| e.from == loop_id && e.handle.as_deref() == Some("body"))
        .map(|e| e.to.clone())
        .collect();
    while let Some(n) = stack.pop() {
        if n == loop_id || !set.insert(n.clone()) {
            continue;
        }
        for e in &graph.edges {
            if e.from == n && e.handle.is_none() && e.to != loop_id {
                stack.push(e.to.clone());
            }
        }
    }
    set
}

/// Find the Loop whose body contains `node` → (loop_id, over key).
fn find_loop_owner(graph: &WorkflowDef, node: &str) -> Option<(String, String)> {
    for n in &graph.nodes {
        if n.node_type == "Loop" && loop_body_set(graph, &n.id).contains(node) {
            let over = n
                .config
                .get("over")
                .and_then(|v| v.as_str())
                .unwrap_or("videos")
                .to_string();
            return Some((n.id.clone(), over));
        }
    }
    None
}

/// The body node feeding `node` via a default edge (its in-body predecessor).
fn body_pred(graph: &WorkflowDef, node: &str, loop_id: &str) -> Option<String> {
    graph
        .edges
        .iter()
        .find(|e| e.to == node && e.handle.is_none() && e.from != loop_id)
        .map(|e| e.from.clone())
}

/// The node feeding the loop from outside its body (the pre-loop context source).
fn loop_feeder(
    graph: &WorkflowDef,
    loop_id: &str,
    body: &std::collections::HashSet<String>,
) -> Option<String> {
    graph
        .edges
        .iter()
        .find(|e| e.to == loop_id && e.handle.is_none() && !body.contains(&e.from))
        .map(|e| e.from.clone())
}

/// `node` + the body nodes reachable from it (default edges, stop at the loop).
fn body_descendants(graph: &WorkflowDef, node: &str, loop_id: &str) -> Vec<String> {
    let mut set = std::collections::HashSet::new();
    let mut stack = vec![node.to_string()];
    while let Some(n) = stack.pop() {
        if n == loop_id || !set.insert(n.clone()) {
            continue;
        }
        for e in &graph.edges {
            if e.from == n && e.handle.is_none() && e.to != loop_id {
                stack.push(e.to.clone());
            }
        }
    }
    set.into_iter().collect()
}

/// Retry a single loop iteration's body from `step_id` (`node#<iter>`): restore
/// that iteration's context at the point before the node, reset its downstream
/// iteration steps, and re-run just that item's body.
async fn retry_iteration(
    st: AppState,
    run_id: String,
    step_id: String,
) -> Result<Json<Value>, AppError> {
    let hash = step_id.find('#').unwrap();
    let node = step_id[..hash].to_string();
    let iter_seg = &step_id[hash + 1..];
    let iter: usize = iter_seg
        .split('#')
        .next()
        .unwrap_or("")
        .parse()
        .map_err(|_| AppError::bad_request("không hỗ trợ chạy lại vòng lặp lồng nhau"))?;

    let graph_json = st
        .services
        .db
        .run_graph(&run_id)
        .await?
        .ok_or_else(|| AppError::not_found("run not found"))?;
    let graph: WorkflowDef = serde_json::from_str(&graph_json).map_err(AppError::internal)?;
    let (loop_id, over) = find_loop_owner(&graph, &node)
        .ok_or_else(|| AppError::bad_request("không tìm thấy vòng lặp chứa khối này"))?;
    let body = loop_body_set(&graph, &loop_id);

    // Restore the iteration context at the point just before `node` runs.
    let parse_ctx = |j: String| {
        RunContext::from_json(&serde_json::from_str::<Value>(&j).unwrap_or(Value::Null))
    };
    let mut ctx = if let Some(pred) = body_pred(&graph, &node, &loop_id) {
        let psid = format!("{pred}#{iter}");
        match st.services.db.step_ctx(&run_id, &psid).await? {
            Some(j) => parse_ctx(j),
            None => {
                return Err(AppError::bad_request(format!(
                    "khối trước '{pred}' của vòng {iter} chưa hoàn thành"
                )))
            }
        }
    } else {
        // `node` is the body start → restore the loop's pre-context and isolate
        // this iteration's single item.
        let mut c = match loop_feeder(&graph, &loop_id, &body) {
            Some(f) => match st.services.db.step_ctx(&run_id, &f).await? {
                Some(j) => parse_ctx(j),
                None => RunContext::default(),
            },
            None => RunContext::default(),
        };
        let arr = c
            .get(&over)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let item = arr
            .get(iter.saturating_sub(1))
            .cloned()
            .ok_or_else(|| AppError::bad_request("không tìm thấy mục để chạy lại"))?;
        c.set(&over, Value::Array(vec![item]));
        c
    };

    // Reset this iteration's steps from `node` onward, then re-run the body item.
    let reset: Vec<String> = body_descendants(&graph, &node, &loop_id)
        .into_iter()
        .map(|n| format!("{n}#{iter}"))
        .collect();
    st.services.db.reset_steps(&run_id, &reset).await?;
    st.services
        .db
        .set_run_status(&run_id, "running", None)
        .await?;

    let services = st.services.clone();
    let registry = st.registry.clone();
    let rid = run_id.clone();
    let suffix = format!("#{iter}");
    spawn_tracked(st.running.clone(), run_id.clone(), async move {
        let obs = DbObserver {
            db: services.db.clone(),
            run_id: rid.clone(),
        };
        match crate::workflow::execute_from(
            &graph,
            &registry,
            &mut ctx,
            &obs,
            &node,
            &suffix,
            Some(loop_id),
        )
        .await
        {
            Ok(_) => {
                let _ = services.db.set_run_status(&rid, "done", None).await;
            }
            Err(e) => {
                let _ = services
                    .db
                    .set_run_status(&rid, "failed", Some(&format!("{e:#}")))
                    .await;
            }
        }
    });
    Ok(Json(json!({ "run_id": run_id })))
}

/// Cancel a running/queued run: abort its task (killing in-flight ffmpeg) and
/// mark it cancelled.
async fn cancel_run(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<StatusCode, AppError> {
    if let Some(handle) = st.running.lock().unwrap().remove(&id) {
        handle.abort();
    }
    st.services.db.cancel_run(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Clear finished runs (done/failed/cancelled) from history.
async fn clear_runs(State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let deleted = st.services.db.clear_finished_runs().await?;
    Ok(Json(json!({ "deleted": deleted })))
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
