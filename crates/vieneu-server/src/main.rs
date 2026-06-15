//! vieneu-server — HTTP API for the VieNeu-TTS v3-Turbo Rust engine.

mod api;
mod jobs;
mod pool;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use vieneu_core::ModelSource;

use crate::api::AppState;
use crate::jobs::JobStore;
use crate::pool::EnginePool;

#[derive(Parser, Debug)]
#[command(name = "vieneu-server", about = "VieNeu-TTS v3-Turbo HTTP API (Rust)")]
struct Args {
    /// Address to bind.
    #[arg(long, default_value = "127.0.0.1:8080")]
    addr: String,
    /// Number of engine workers (parallel synthesis). Tune to your machine.
    #[arg(long, default_value_t = 2)]
    workers: usize,
    /// Intra-op threads per engine (0 = let ONNX Runtime decide).
    #[arg(long, default_value_t = 0)]
    threads: usize,
    /// Local model directory (HF-repo layout). Omitted → download from the hub.
    #[arg(long)]
    model_dir: Option<String>,
    /// Local MOSS codec directory (required with --model-dir).
    #[arg(long)]
    codec_dir: Option<String>,
    /// Hugging Face token for gated/private repos.
    #[arg(long, env = "HF_TOKEN")]
    hf_token: Option<String>,
    /// Directory for finished job audio (cleaned on exit). Default: a unique
    /// folder under the system temp dir.
    #[arg(long, env = "VIENEU_CACHE_DIR")]
    cache_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "vieneu_server=info,tower_http=info,ort=warn".into()),
        )
        .init();

    let args = Args::parse();

    let source = match (&args.model_dir, &args.codec_dir) {
        (Some(v), Some(c)) => ModelSource::Local {
            v3_dir: v.into(),
            codec_dir: c.into(),
        },
        (Some(_), None) => {
            anyhow::bail!("--model-dir requires --codec-dir");
        }
        _ => ModelSource::Hub,
    };

    tracing::info!("building pool of {} engine(s)…", args.workers);
    let pool = EnginePool::build(args.workers, source, args.hf_token.clone(), args.threads)?;
    tracing::info!(
        "ready: {} voices, {} Hz, {} worker(s)",
        pool.voices.len(),
        pool.sample_rate,
        pool.size
    );

    let tmp_dir = std::path::PathBuf::from(args.cache_dir.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join(format!("vieneu-{}", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }));
    std::fs::create_dir_all(&tmp_dir)?;
    tracing::info!("audio cache: {}", tmp_dir.display());
    let tmp_dir = Arc::new(tmp_dir);

    let state = AppState {
        pool,
        clones: Arc::new(Mutex::new(HashMap::new())),
        jobs: Arc::new(JobStore::default()),
        tmp_dir: tmp_dir.clone(),
    };

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/v1/info", get(api::info))
        .route("/v1/voices", get(api::list_voices))
        .route("/v1/tts", post(api::tts))
        .route("/v1/clone", post(api::clone_voice))
        .route("/v1/jobs", post(api::create_job))
        .route("/v1/jobs/{id}", get(api::get_job).delete(api::cancel_job))
        .route("/v1/jobs/{id}/download", get(api::download_job))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.addr).await?;
    tracing::info!("listening on http://{}", args.addr);
    // Generated files are kept on disk (not deleted on exit) so they remain
    // available after the app closes.
    axum::serve(listener, app).await?;
    Ok(())
}
