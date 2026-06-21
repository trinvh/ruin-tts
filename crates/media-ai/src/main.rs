//! media-ai (Rust): audio-analysis sidecar for the video-dubbing pipeline.
//! Stateless, file-path based; serves the same `/analyze` + `/health` contract
//! as the Python sidecar so studio's MediaAiClient is unchanged.

mod agegender;
mod analyze;
mod asr;
mod audio;
mod diarize;
mod models;
mod types;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde_json::json;

use crate::analyze::Analyzer;
use crate::types::AnalyzeRequest;

#[derive(Parser)]
#[command(about = "media-ai: audio analysis sidecar (ASR + diarization + age/gender), Rust port")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8099")]
    addr: String,
    #[arg(long, env = "HF_TOKEN")]
    hf_token: Option<String>,
    #[arg(
        long,
        env = "MEDIA_AI_WHISPER_REPO",
        default_value = "ggerganov/whisper.cpp"
    )]
    whisper_repo: String,
    #[arg(
        long,
        env = "MEDIA_AI_WHISPER_MODEL",
        default_value = "ggml-large-v3-turbo.bin"
    )]
    whisper_model: String,
}

struct AppState {
    analyzer: Analyzer,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "media_ai=info,tower_http=info".into()),
        )
        .init();
    let args = Args::parse();

    tracing::info!("nạp model (lần đầu sẽ tải về)…");
    let model_path = models::hf_file(
        &args.whisper_repo,
        &args.whisper_model,
        args.hf_token.clone(),
    )
    .context("tải model whisper")?;
    let asr = asr::Asr::load(model_path.to_string_lossy().as_ref())?;
    let analyzer = Analyzer::new(
        asr,
        diarize::Diarizer::load()?,
        agegender::AgeGenderModel::load()?,
    );

    let state = Arc::new(AppState { analyzer });
    let app = Router::new()
        .route("/health", get(health))
        .route("/analyze", post(analyze_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.addr)
        .await
        .with_context(|| format!("bind {}", args.addr))?;
    tracing::info!("media-ai (rust) lắng nghe trên http://{}", args.addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "impl": "rust" }))
}

async fn analyze_handler(
    State(st): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    if !std::path::Path::new(&req.audio_path).is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": format!("audio not found: {}", req.audio_path) })),
        )
            .into_response();
    }
    // ASR is CPU-heavy → run off the async runtime.
    let res = tokio::task::spawn_blocking(move || {
        st.analyzer
            .analyze(&req.audio_path, req.hint_lang.as_deref(), req.num_speakers)
    })
    .await;
    match res {
        Ok(Ok(out)) => Json(out).into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": e.to_string() })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "detail": format!("task: {e}") })),
        )
            .into_response(),
    }
}
