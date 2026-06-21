//! HTTP API: state, request/response types, and handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    body::Bytes,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use ndarray::Array2;
use serde::{Deserialize, Serialize};
use vieneu_core::{InferOptions, OutputFormat, SamplingParams, VoiceSelection};

use crate::jobs::{JobStore, JobView};
use crate::pool::{EnginePool, VoiceInfo};

#[derive(Clone)]
pub struct AppState {
    pub pool: Arc<EnginePool>,
    pub clones: Arc<Mutex<HashMap<String, Array2<i64>>>>,
    pub jobs: Arc<JobStore>,
    /// Directory where finished job audio is written (kept off the heap).
    pub tmp_dir: Arc<std::path::PathBuf>,
}

/// Common synthesis parameters shared by the sync and async endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    /// Built-in preset voice id (ignored if `ref_id` is set).
    #[serde(default)]
    pub voice: Option<String>,
    /// A cloned-voice handle returned by `/v1/clone`.
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(default = "default_emotion")]
    pub emotion: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub repetition_penalty: Option<f32>,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub max_new_frames: Option<usize>,
    #[serde(default)]
    pub silence_p: Option<f32>,
    #[serde(default)]
    pub paragraph_silence_p: Option<f32>,
    #[serde(default)]
    pub crossfade_p: Option<f32>,
    /// Output format: "wav" (default) or "mp3".
    #[serde(default)]
    pub format: Option<String>,
}

fn default_emotion() -> String {
    "natural".to_string()
}

fn parse_format(opt: &Option<String>) -> OutputFormat {
    opt.as_deref()
        .and_then(OutputFormat::parse)
        .unwrap_or(OutputFormat::Wav)
}

impl TtsRequest {
    /// Resolve into engine options, looking up a cloned voice if requested.
    fn into_options(
        self,
        clones: &Mutex<HashMap<String, Array2<i64>>>,
    ) -> Result<InferOptions, AppError> {
        let voice = if let Some(ref_id) = &self.ref_id {
            let map = clones.lock().unwrap();
            let codes = map
                .get(ref_id)
                .ok_or_else(|| AppError::bad_request(format!("unknown ref_id '{ref_id}'")))?;
            VoiceSelection::CloneCodes(codes.clone())
        } else if let Some(name) = self.voice.clone() {
            VoiceSelection::Preset(name)
        } else {
            VoiceSelection::Default
        };

        let d = SamplingParams::default();
        let sampling = SamplingParams {
            temperature: self.temperature.unwrap_or(d.temperature),
            top_k: self.top_k.unwrap_or(d.top_k),
            top_p: self.top_p.unwrap_or(d.top_p),
            repetition_penalty: self.repetition_penalty.unwrap_or(d.repetition_penalty),
        };
        let def = InferOptions::default();
        Ok(InferOptions {
            voice,
            emotion: self.emotion,
            sampling,
            max_new_frames: self.max_new_frames.unwrap_or(def.max_new_frames),
            max_chars: self.max_chars.unwrap_or(def.max_chars),
            silence_p: self.silence_p.unwrap_or(def.silence_p),
            paragraph_silence_p: self
                .paragraph_silence_p
                .unwrap_or(def.paragraph_silence_p),
            crossfade_p: self.crossfade_p.unwrap_or(def.crossfade_p),
        })
    }
}

// ── Handlers ────────────────────────────────────────────────────────────────

pub async fn health() -> &'static str {
    "ok"
}

pub async fn list_voices(State(st): State<AppState>) -> Json<Vec<VoiceInfo>> {
    Json(st.pool.voices.clone())
}

#[derive(Serialize)]
pub struct InfoResponse {
    pub sample_rate: u32,
    pub pool_size: usize,
    pub voices: usize,
}

pub async fn info(State(st): State<AppState>) -> Json<InfoResponse> {
    Json(InfoResponse {
        sample_rate: st.pool.sample_rate,
        pool_size: st.pool.size,
        voices: st.pool.voices.len(),
    })
}

/// Synchronous synthesis → returns a WAV body.
pub async fn tts(
    State(st): State<AppState>,
    Json(req): Json<TtsRequest>,
) -> Result<Response, AppError> {
    if req.text.trim().is_empty() {
        return Err(AppError::bad_request("text is empty"));
    }
    let format = parse_format(&req.format);
    let opts = req.clone().into_options(&st.clones)?;
    let text = req.text.clone();
    let sr = st.pool.sample_rate;

    let wav = st
        .pool
        .with_engine(move |e| e.infer(&text, &opts))
        .await
        .map_err(AppError::internal)?;

    let (bytes, content_type) =
        vieneu_core::encode(&wav, sr, format).map_err(AppError::internal)?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        Bytes::from(bytes),
    )
        .into_response())
}

#[derive(Serialize)]
pub struct CloneResponse {
    pub ref_id: String,
    pub frames: usize,
}

/// Clone a voice from an uploaded reference clip → returns a `ref_id`.
pub async fn clone_voice(
    State(st): State<AppState>,
    mut mp: Multipart,
) -> Result<Json<CloneResponse>, AppError> {
    let mut data: Option<(String, Vec<u8>)> = None;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
    {
        if field.name() == Some("file") || data.is_none() {
            let fname = field.file_name().unwrap_or("ref.wav").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;
            data = Some((fname, bytes.to_vec()));
        }
    }
    let (fname, bytes) = data.ok_or_else(|| AppError::bad_request("no file field"))?;

    let ext = std::path::Path::new(&fname)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");
    let tmp = std::env::temp_dir().join(format!("vieneu_ref_{}.{ext}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, &bytes).map_err(AppError::internal)?;

    let tmp2 = tmp.clone();
    let codes = st
        .pool
        .with_engine(move |e| e.encode_reference(&tmp2))
        .await
        .map_err(AppError::internal)?;
    let _ = std::fs::remove_file(&tmp);

    let frames = codes.nrows();
    let ref_id = uuid::Uuid::new_v4().to_string();
    st.clones.lock().unwrap().insert(ref_id.clone(), codes);
    Ok(Json(CloneResponse { ref_id, frames }))
}

#[derive(Serialize)]
pub struct JobCreated {
    pub job_id: String,
}

/// Submit a long synthesis job (good for whole chapters); poll `/v1/jobs/:id`.
pub async fn create_job(
    State(st): State<AppState>,
    Json(req): Json<TtsRequest>,
) -> Result<Json<JobCreated>, AppError> {
    if req.text.trim().is_empty() {
        return Err(AppError::bad_request("text is empty"));
    }
    let format = parse_format(&req.format);
    let opts = req.clone().into_options(&st.clones)?;
    let (job_id, cancel) = st.jobs.create();
    let jobs = st.jobs.clone();
    let pool = st.pool.clone();
    let tmp_dir = st.tmp_dir.clone();
    let text = req.text.clone();
    let sr = pool.sample_rate;
    let id = job_id.clone();

    tokio::spawn(async move {
        jobs.mark_running(&id);
        let token = cancel.clone();
        let res = pool
            .with_engine(move |e| e.infer_cancellable(&text, &opts, &token))
            .await;
        match res {
            Ok(wav) => {
                let dur = wav.len() as f32 / sr as f32;
                match vieneu_core::encode(&wav, sr, format) {
                    Ok((bytes, ct)) => {
                        let path = tmp_dir.join(format!("{id}.{}", format.extension()));
                        match std::fs::write(&path, &bytes) {
                            Ok(()) => jobs.mark_done(&id, path, ct, format.extension(), dur),
                            Err(e) => jobs.mark_failed(&id, e.to_string()),
                        }
                    }
                    Err(e) => jobs.mark_failed(&id, e.to_string()),
                }
            }
            // A cancelled run already set the job's status; only real errors fail it.
            Err(e) if vieneu_core::is_cancelled(&e) => {}
            Err(e) => jobs.mark_failed(&id, e.to_string()),
        }
    });

    Ok(Json(JobCreated { job_id }))
}

/// Cancel a running or queued job.
pub async fn cancel_job(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if st.jobs.request_cancel(&id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("job not found"))
    }
}

pub async fn get_job(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobView>, AppError> {
    st.jobs
        .view(&id)
        .map(Json)
        .ok_or_else(|| AppError::not_found("job not found"))
}

pub async fn download_job(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let (path, content_type, ext) = st
        .jobs
        .audio_path(&id)
        .ok_or_else(|| AppError::not_found("job not ready or not found"))?;
    let bytes = std::fs::read(&path).map_err(AppError::internal)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{id}.{ext}\""),
            ),
        ],
        Bytes::from(bytes),
    )
        .into_response())
}

// ── Error type ──────────────────────────────────────────────────────────────

pub struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.into(),
        }
    }
    fn internal(e: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("request error: {}", self.message);
        }
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}
