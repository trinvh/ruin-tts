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
const MODELS_REPO: &str = "trinvh/ruin-media-ai";

#[derive(Parser, Clone)]
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
    prog: &models::DownloadProgress,
) -> Option<std::path::PathBuf> {
    if let Some(p) = path {
        return Some(std::path::PathBuf::from(p));
    }
    let repo = repo.as_ref()?;
    match models::hf_file_with_progress(repo, model, token, prog) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!("không tải được model {label} ({repo}/{model}): {e} — bỏ qua");
            None
        }
    }
}

/// Download (with progress) + load every model and assemble the analyzer. Runs
/// off the HTTP thread so `/progress` + `/health` stay responsive meanwhile.
fn build_analyzer(args: &Args, prog: &models::DownloadProgress) -> Result<Analyzer> {
    let model_path = models::hf_file_with_progress(
        &args.whisper_repo,
        &args.whisper_model,
        args.hf_token.clone(),
        prog,
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
        prog,
    );
    let agegender = agegender::AgeGenderModel::load(agegender_path.as_deref())?;
    let embed_path = resolve_model(
        &args.embed_path,
        &args.embed_repo,
        &args.embed_model,
        tok.clone(),
        "speaker-embedding",
        prog,
    );
    let embedder = embed::Embedder::load(embed_path.as_deref())?;
    let segment_path = resolve_model(
        &args.segment_path,
        &args.segment_repo,
        &args.segment_model,
        tok.clone(),
        "segmentation",
        prog,
    );
    let segmenter = segment::Segmenter::load(segment_path.as_deref())?;
    let separate_path = resolve_model(
        &args.separate_path,
        &args.separate_repo,
        &args.separate_model,
        tok,
        "separation",
        prog,
    );
    let separator = separate::Separator::load(separate_path.as_deref())?;
    let threshold = args.diarize_threshold.unwrap_or(diarize::DEFAULT_THRESHOLD);
    Ok(Analyzer::new(
        asr, embedder, agegender, segmenter, separator, threshold,
    ))
}

struct AppState {
    // Built in the background while the server already serves /health + /progress.
    analyzer: std::sync::RwLock<Option<Arc<Analyzer>>>,
    progress: models::DownloadProgress,
}

/// When launched by the desktop shell (`DIE_WITH_PARENT=1` + a piped stdin), exit
/// as soon as that stdin closes — i.e. when the parent dies — so we never linger
/// holding the port. No-op for standalone runs (env unset).
fn die_with_parent() {
    if std::env::var_os("DIE_WITH_PARENT").is_none() {
        return;
    }
    std::thread::spawn(|| {
        use std::io::Read;
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => std::process::exit(0),
                Ok(_) => {}
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    die_with_parent();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "media_ai=info,tower_http=info".into()),
        )
        .init();
    let args = Args::parse();

    // Bind first so the desktop onboarding can poll /progress + /health while the
    // (multi-GB, first-run) models download in the background.
    let listener = tokio::net::TcpListener::bind(&args.addr)
        .await
        .with_context(|| format!("bind {}", args.addr))?;
    tracing::info!("media-ai (rust) lắng nghe trên http://{}", args.addr);

    let state = Arc::new(AppState {
        analyzer: std::sync::RwLock::new(None),
        progress: models::DownloadProgress::default(),
    });
    {
        let state = state.clone();
        let args = args.clone();
        std::thread::spawn(move || {
            tracing::info!("nạp model (lần đầu sẽ tải về)…");
            match build_analyzer(&args, &state.progress) {
                Ok(a) => {
                    *state.analyzer.write().unwrap() = Some(Arc::new(a));
                    state.progress.set_done();
                    tracing::info!("media-ai sẵn sàng");
                }
                Err(e) => tracing::error!("nạp model thất bại: {e}"),
            }
        });
    }

    let app = Router::new()
        .route("/health", get(health))
        .route("/progress", get(progress_handler))
        .route("/analyze", post(analyze_handler))
        // The desktop webview reads /health + /progress directly (onboarding) —
        // needs CORS, like vieneu-server + studio-server.
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let ready = st.analyzer.read().unwrap().is_some();
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        code,
        Json(json!({ "status": if ready { "ok" } else { "loading" }, "impl": "rust" })),
    )
}

async fn progress_handler(State(st): State<Arc<AppState>>) -> Json<models::DownloadStatus> {
    Json(st.progress.snapshot())
}

async fn analyze_handler(
    State(st): State<Arc<AppState>>,
    Json(req): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    let Some(analyzer) = st.analyzer.read().unwrap().clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "detail": "đang tải model — thử lại sau" })),
        )
            .into_response();
    };
    if !std::path::Path::new(&req.audio_path).is_file() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "detail": format!("audio not found: {}", req.audio_path) })),
        )
            .into_response();
    }
    // ASR is CPU-heavy → run off the async runtime.
    let res = tokio::task::spawn_blocking(move || {
        analyzer.analyze(
            &req.audio_path,
            req.hint_lang.as_deref(),
            req.num_speakers,
            req.max_speakers,
        )
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
