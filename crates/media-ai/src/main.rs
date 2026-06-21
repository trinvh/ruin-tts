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
mod segment;
mod separate;
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

/// Default HF repo for the project-exported ONNX models (speaker embedding +
/// age/gender). Populate it once with `make upload-models` (see
/// tools/upload-models.sh); a missing repo just degrades gracefully.
const MODELS_REPO: &str = "trinvhco/ruin-media-ai";

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
    /// HF repo holding the exported age/gender ONNX. Defaults to the project's
    /// model repo; download failure degrades gracefully (no age/gender).
    #[arg(long, env = "MEDIA_AI_AGEGENDER_REPO", default_value = MODELS_REPO)]
    agegender_repo: Option<String>,
    #[arg(
        long,
        env = "MEDIA_AI_AGEGENDER_MODEL",
        default_value = "agegender.onnx"
    )]
    agegender_model: String,
    /// Local path to the age/gender ONNX (overrides the repo download).
    #[arg(long, env = "MEDIA_AI_AGEGENDER_PATH")]
    agegender_path: Option<String>,
    /// HF repo holding the exported speaker-embedding ONNX (WavLM-SV).
    #[arg(long, env = "MEDIA_AI_EMBED_REPO", default_value = MODELS_REPO)]
    embed_repo: Option<String>,
    #[arg(
        long,
        env = "MEDIA_AI_EMBED_MODEL",
        default_value = "speaker-embedding.onnx"
    )]
    embed_model: String,
    /// Local path to the speaker-embedding ONNX (overrides the repo download).
    #[arg(long, env = "MEDIA_AI_EMBED_PATH")]
    embed_path: Option<String>,
    /// HF repo holding the pyannote segmentation ONNX (for overlap detection).
    /// Defaults to sherpa-onnx's public (non-gated) pre-export.
    #[arg(
        long,
        env = "MEDIA_AI_SEGMENT_REPO",
        default_value = "csukuangfj/sherpa-onnx-pyannote-segmentation-3-0"
    )]
    segment_repo: Option<String>,
    #[arg(long, env = "MEDIA_AI_SEGMENT_MODEL", default_value = "model.onnx")]
    segment_model: String,
    /// Local path to the pyannote segmentation ONNX (overrides the repo).
    #[arg(long, env = "MEDIA_AI_SEGMENT_PATH")]
    segment_path: Option<String>,
    /// HF repo holding the ConvTasNet separation ONNX (per-speaker text in
    /// overlaps). Defaults to the project repo; missing → no overlap separation.
    #[arg(long, env = "MEDIA_AI_SEPARATE_REPO", default_value = MODELS_REPO)]
    separate_repo: Option<String>,
    #[arg(
        long,
        env = "MEDIA_AI_SEPARATE_MODEL",
        default_value = "separation.onnx"
    )]
    separate_model: String,
    /// Local path to the separation ONNX (overrides the repo download).
    #[arg(long, env = "MEDIA_AI_SEPARATE_PATH")]
    separate_path: Option<String>,
    /// Cosine-similarity threshold for diarization clustering.
    #[arg(long, env = "MEDIA_AI_DIARIZE_THRESHOLD")]
    diarize_threshold: Option<f32>,
}

/// Resolve an optional ONNX: explicit local path wins; else try the HF repo.
/// A download failure is **non-fatal** — it logs and returns `None`, so the
/// sidecar still starts (just without that capability) when a default repo isn't
/// populated yet.
fn resolve_model(
    path: &Option<String>,
    repo: &Option<String>,
    model: &str,
    token: Option<String>,
    label: &str,
) -> Option<std::path::PathBuf> {
    if let Some(p) = path {
        return Some(std::path::PathBuf::from(p));
    }
    let repo = repo.as_ref()?;
    match models::hf_file(repo, model, token) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!("không tải được model {label} ({repo}/{model}): {e} — bỏ qua");
            None
        }
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
    let tok = args.hf_token.clone();
    let agegender_path = resolve_model(
        &args.agegender_path,
        &args.agegender_repo,
        &args.agegender_model,
        tok.clone(),
        "age/gender",
    );
    let agegender = agegender::AgeGenderModel::load(agegender_path.as_deref())?;
    let embed_path = resolve_model(
        &args.embed_path,
        &args.embed_repo,
        &args.embed_model,
        tok.clone(),
        "speaker-embedding",
    );
    let embedder = embed::Embedder::load(embed_path.as_deref())?;
    let segment_path = resolve_model(
        &args.segment_path,
        &args.segment_repo,
        &args.segment_model,
        tok,
        "segmentation",
    );
    let segmenter = segment::Segmenter::load(segment_path.as_deref())?;
    let separate_path = resolve_model(
        &args.separate_path,
        &args.separate_repo,
        &args.separate_model,
        args.hf_token.clone(),
        "separation",
    );
    let separator = separate::Separator::load(separate_path.as_deref())?;
    let threshold = args.diarize_threshold.unwrap_or(diarize::DEFAULT_THRESHOLD);
    let analyzer = Analyzer::new(asr, embedder, agegender, segmenter, separator, threshold);

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
