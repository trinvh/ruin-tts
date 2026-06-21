//! media-ai (Rust): audio-analysis sidecar for the video-dubbing pipeline.
//! Stateless, file-path based; serves the same `/analyze` + `/health` contract
//! as the Python sidecar so studio's MediaAiClient is unchanged.

mod agegender;
mod analyze;
mod asr;
mod audio;
mod cluster;
mod diarize;
mod embed;
mod models;
mod onnx;
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
    /// HF repo holding the exported age/gender ONNX (omit to disable age/gender).
    #[arg(long, env = "MEDIA_AI_AGEGENDER_REPO")]
    agegender_repo: Option<String>,
    #[arg(long, env = "MEDIA_AI_AGEGENDER_MODEL", default_value = "model.onnx")]
    agegender_model: String,
    /// Local path to the age/gender ONNX (overrides the repo download).
    #[arg(long, env = "MEDIA_AI_AGEGENDER_PATH")]
    agegender_path: Option<String>,
    /// HF repo holding the exported speaker-embedding ONNX (WavLM-SV).
    #[arg(long, env = "MEDIA_AI_EMBED_REPO")]
    embed_repo: Option<String>,
    #[arg(long, env = "MEDIA_AI_EMBED_MODEL", default_value = "model.onnx")]
    embed_model: String,
    /// Local path to the speaker-embedding ONNX (overrides the repo download).
    #[arg(long, env = "MEDIA_AI_EMBED_PATH")]
    embed_path: Option<String>,
    /// Cosine-similarity threshold for diarization clustering.
    #[arg(long, env = "MEDIA_AI_DIARIZE_THRESHOLD")]
    diarize_threshold: Option<f32>,
}

/// Resolve the optional age/gender ONNX: explicit local path, else an HF repo
/// download, else None (disabled).
fn resolve_agegender(args: &Args) -> Result<Option<std::path::PathBuf>> {
    if let Some(p) = &args.agegender_path {
        return Ok(Some(std::path::PathBuf::from(p)));
    }
    match &args.agegender_repo {
        Some(repo) => Ok(Some(
            models::hf_file(repo, &args.agegender_model, args.hf_token.clone())
                .context("tải model age/gender")?,
        )),
        None => Ok(None),
    }
}

/// Resolve the optional speaker-embedding ONNX (WavLM-SV): explicit local path,
/// else an HF repo download, else None (diarization → single speaker).
fn resolve_embed(args: &Args) -> Result<Option<std::path::PathBuf>> {
    if let Some(p) = &args.embed_path {
        return Ok(Some(std::path::PathBuf::from(p)));
    }
    match &args.embed_repo {
        Some(repo) => Ok(Some(
            models::hf_file(repo, &args.embed_model, args.hf_token.clone())
                .context("tải model speaker-embedding")?,
        )),
        None => Ok(None),
    }
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
    let agegender_path = resolve_agegender(&args)?;
    let agegender = agegender::AgeGenderModel::load(agegender_path.as_deref())?;
    let embed_path = resolve_embed(&args)?;
    let embedder = embed::Embedder::load(embed_path.as_deref())?;
    let threshold = args.diarize_threshold.unwrap_or(diarize::DEFAULT_THRESHOLD);
    let analyzer = Analyzer::new(asr, embedder, agegender, threshold);

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
