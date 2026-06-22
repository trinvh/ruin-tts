//! HTTP endpoints for video dubbing. Long steps (extract/analyze/translate/
//! synthesize/build/export) run as tracked background tasks: the handler sets a
//! `*-ing` status and returns immediately; the UI polls the project until it
//! reaches the next state (or `failed` with an error).

use std::path::Path;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Multipart, Path as AxPath, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::dub::pipeline;
use crate::nodes::Services;
use crate::server::{spawn_tracked, AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dub/projects", get(list_projects).post(create_project))
        .route(
            "/api/dub/projects/{id}",
            get(get_project).delete(delete_project),
        )
        .route("/api/dub/projects/{id}/settings", put(update_settings))
        .route("/api/dub/projects/{id}/video", get(serve_video))
        .route("/api/dub/projects/{id}/info", get(media_info))
        .route("/api/dub/projects/{id}/extract", post(step_extract))
        .route("/api/dub/projects/{id}/analyze", post(step_analyze))
        .route("/api/dub/projects/{id}/translate", post(step_translate))
        .route("/api/dub/projects/{id}/synthesize", post(step_synthesize))
        .route("/api/dub/projects/{id}/reshorten", post(step_reshorten))
        .route("/api/dub/projects/{id}/build", post(step_build))
        .route("/api/dub/projects/{id}/export", post(step_export))
        .route("/api/dub/projects/{id}/cancel", post(cancel))
        .route("/api/dub/segments/{id}", put(update_segment))
        .route("/api/dub/segments/{id}/offset", put(set_segment_offset))
        .route(
            "/api/dub/projects/{id}/speakers/{speaker}/voice",
            put(set_speaker_voice),
        )
        .route("/api/dub/projects/{id}/video-offset", put(set_video_offset))
        .route("/api/dub/projects/{id}/overlays", post(create_overlay))
        .route(
            "/api/dub/overlays/{oid}",
            put(update_overlay).delete(delete_overlay),
        )
        .route("/api/dub/overlays/{oid}/image", get(serve_overlay_image))
}

/// Running-map key for a dubbing task (namespaced so it can't collide with run ids).
fn dub_key(id: &str) -> String {
    format!("dub:{id}")
}

// ── Projects ──────────────────────────────────────────────────────────────────
async fn list_projects(State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let projects = st.services.db.list_dub_projects().await?;
    Ok(Json(json!({ "projects": projects })))
}

#[derive(Deserialize)]
struct CreateProject {
    name: String,
    video_path: String,
    #[serde(default)]
    gemini_model: Option<String>,
}

async fn create_project(
    State(st): State<AppState>,
    Json(body): Json<CreateProject>,
) -> Result<Json<Value>, AppError> {
    if body.video_path.trim().is_empty() {
        return Err(AppError::bad_request("chưa chọn video"));
    }
    if !Path::new(&body.video_path).is_file() {
        return Err(AppError::bad_request(format!(
            "không tìm thấy file video: {}",
            body.video_path
        )));
    }
    let model = match body.gemini_model {
        Some(m) if !m.trim().is_empty() => m,
        _ => st.services.config.read().await.gemini_model.clone(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let name = if body.name.trim().is_empty() {
        Path::new(&body.video_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Dự án lồng tiếng")
            .to_string()
    } else {
        body.name
    };
    st.services
        .db
        .create_dub_project(&id, &name, &body.video_path, &model)
        .await?;
    let project = st.services.db.get_dub_project(&id).await?;
    Ok(Json(json!({ "project": project })))
}

async fn get_project(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    let project = st
        .services
        .db
        .get_dub_project(&id)
        .await?
        .ok_or_else(|| AppError::not_found("không tìm thấy dự án"))?;
    let segments = st.services.db.get_dub_segments(&id).await?;
    let speakers = st.services.db.get_dub_speakers(&id).await?;
    let overlays = st.services.db.list_dub_overlays(&id).await?;
    Ok(Json(json!({
        "project": project,
        "segments": segments,
        "speakers": speakers,
        "overlays": overlays,
    })))
}

async fn delete_project(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    if let Some(h) = st.running.lock().unwrap().remove(&dub_key(&id)) {
        h.abort();
    }
    st.services.db.delete_dub_project(&id).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct UpdateSettings {
    name: String,
    gemini_model: String,
    original_volume: f64,
    #[serde(default = "default_vn_volume")]
    vn_volume: f64,
    speed_cap: f64,
    #[serde(default)]
    burn_subtitles: bool,
    #[serde(default)]
    blur_subtitle: bool,
    #[serde(default)]
    blur_x: f64,
    #[serde(default = "default_blur_y")]
    blur_y: f64,
    #[serde(default = "default_blur_w")]
    blur_w: f64,
    #[serde(default = "default_blur_h")]
    blur_h: f64,
    #[serde(default = "default_sub_y")]
    sub_y: f64,
    #[serde(default = "default_sub_size")]
    sub_size: f64,
    #[serde(default = "default_sub_color")]
    sub_color: String,
    #[serde(default)]
    sub_bilingual: bool,
    #[serde(default = "default_true")]
    video_enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_sub_y() -> f64 {
    0.9
}

fn default_sub_size() -> f64 {
    30.0
}

fn default_sub_color() -> String {
    "#ffffff".to_string()
}

fn default_vn_volume() -> f64 {
    1.0
}

/// Accept only a `#RRGGBB` hex colour; fall back to white on anything else so a
/// malformed value can never reach the ffmpeg `force_style` filter.
fn sanitize_hex_color(c: &str) -> String {
    let t = c.trim();
    let ok = t.len() == 7 && t.starts_with('#') && t[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        t.to_lowercase()
    } else {
        default_sub_color()
    }
}

fn default_blur_y() -> f64 {
    0.84
}
fn default_blur_w() -> f64 {
    1.0
}
fn default_blur_h() -> f64 {
    0.14
}

async fn update_settings(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<UpdateSettings>,
) -> Result<Json<Value>, AppError> {
    st.services
        .db
        .update_dub_settings(
            &id,
            &b.name,
            &b.gemini_model,
            b.original_volume.clamp(0.0, 1.0),
            b.vn_volume.clamp(0.0, 1.0),
            b.speed_cap.clamp(1.0, 2.5),
            b.burn_subtitles,
            b.blur_subtitle,
            (
                b.blur_x.clamp(0.0, 0.99),
                b.blur_y.clamp(0.0, 0.99),
                b.blur_w.clamp(0.01, 1.0),
                b.blur_h.clamp(0.01, 1.0),
            ),
            b.sub_y.clamp(0.0, 1.0),
            b.sub_size.clamp(8.0, 120.0),
            &sanitize_hex_color(&b.sub_color),
            b.sub_bilingual,
            b.video_enabled,
        )
        .await?;
    let project = st.services.db.get_dub_project(&id).await?;
    Ok(Json(json!({ "project": project })))
}

// ── Steps ─────────────────────────────────────────────────────────────────────

/// Run a pipeline step in the background: set `busy`, spawn, then set `done`.
///
/// On failure we do NOT collapse the project to a single "failed" state (which
/// would lose how far the pipeline got and force a restart). Instead we revert
/// to `from` — the step's prerequisite, i.e. the last completed checkpoint — and
/// attach the error. Completed steps stay done; the failed step is simply the
/// next runnable one (the UI flags it via the persisted error). Returns
/// immediately so the UI can poll.
async fn run_step<F, Fut>(
    st: AppState,
    id: String,
    from: &str,
    busy: &str,
    done: &str,
    f: F,
) -> Result<Json<Value>, AppError>
where
    F: FnOnce(Arc<Services>, String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    if st.services.db.get_dub_project(&id).await?.is_none() {
        return Err(AppError::not_found("không tìm thấy dự án"));
    }
    // Setting `busy` clears any previous error for this project.
    st.services.db.set_dub_status(&id, busy, None).await?;
    let services = st.services.clone();
    let rid = id.clone();
    let done = done.to_string();
    let from = from.to_string();
    spawn_tracked(st.running.clone(), dub_key(&id), async move {
        match f(services.clone(), rid.clone()).await {
            Ok(_) => {
                let _ = services.db.set_dub_status(&rid, &done, None).await;
            }
            Err(e) => {
                let _ = services
                    .db
                    .set_dub_status(&rid, &from, Some(&format!("{e:#}")))
                    .await;
            }
        }
    });
    Ok(Json(json!({ "status": busy })))
}

async fn step_extract(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "created",
        "extracting",
        "extracted",
        |s, id| async move { pipeline::extract_audio(&s, &id).await },
    )
    .await
}

async fn step_analyze(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "extracted",
        "analyzing",
        "analyzed",
        |s, id| async move { pipeline::analyze(&s, &id).await },
    )
    .await
}

async fn step_translate(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "analyzed",
        "translating",
        "translated",
        |s, id| async move { pipeline::translate(&s, &id).await },
    )
    .await
}

async fn step_synthesize(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "translated",
        "synthesizing",
        "synthesized",
        |s, id| async move { pipeline::synthesize(&s, &id).await },
    )
    .await
}

async fn step_reshorten(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "synthesized",
        "synthesizing",
        "synthesized",
        |s, id| async move { pipeline::reshorten_long(&s, &id).await.map(|_| ()) },
    )
    .await
}

async fn step_build(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(
        st,
        id,
        "synthesized",
        "building",
        "built",
        |s, id| async move { pipeline::build_track(&s, &id).await },
    )
    .await
}

async fn step_export(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    run_step(st, id, "built", "exporting", "done", |s, id| async move {
        pipeline::export(&s, &id).await
    })
    .await
}

/// Map a busy (`*-ing`) status back to the checkpoint a step started from, so
/// cancelling mid-step leaves the project at its last completed state (not a
/// dead-end "cancelled") and the step stays runnable.
fn busy_from(status: &str) -> &'static str {
    match status {
        "extracting" => "created",
        "analyzing" => "extracted",
        "translating" => "analyzed",
        "synthesizing" => "translated",
        "building" => "synthesized",
        "exporting" => "built",
        _ => "created",
    }
}

async fn cancel(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    if let Some(h) = st.running.lock().unwrap().remove(&dub_key(&id)) {
        h.abort();
    }
    // Revert to the checkpoint the in-flight step started from.
    let revert = match st.services.db.get_dub_project(&id).await? {
        Some(p) => busy_from(&p.status),
        None => "created",
    };
    st.services.db.set_dub_status(&id, revert, None).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Segment / speaker edits ────────────────────────────────────────────────────
#[derive(Deserialize)]
struct UpdateSegment {
    text_vi: String,
    #[serde(default)]
    voice: Option<String>,
}

async fn update_segment(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<UpdateSegment>,
) -> Result<Json<Value>, AppError> {
    let voice = b.voice.as_deref().filter(|v| !v.trim().is_empty());
    st.services
        .db
        .update_dub_segment(&id, &b.text_vi, voice)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SetOffset {
    offset_s: f64,
}

async fn set_segment_offset(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<SetOffset>,
) -> Result<Json<Value>, AppError> {
    st.services
        .db
        .set_dub_segment_offset(&id, b.offset_s)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SetVoice {
    #[serde(default)]
    voice: Option<String>,
}

async fn set_speaker_voice(
    State(st): State<AppState>,
    AxPath((id, speaker)): AxPath<(String, String)>,
    Json(b): Json<SetVoice>,
) -> Result<Json<Value>, AppError> {
    let voice = b.voice.as_deref().filter(|v| !v.trim().is_empty());
    st.services
        .db
        .set_dub_speaker_voice(&id, &speaker, voice)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn media_info(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    let project = st
        .services
        .db
        .get_dub_project(&id)
        .await?
        .ok_or_else(|| AppError::not_found("không tìm thấy dự án"))?;
    let info = crate::media::probe_media_info(Path::new(&project.video_path)).await?;
    Ok(Json(info))
}

// ── Original video streaming (for preview; path is one we stored, not arbitrary) ─
async fn serve_video(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Response, AppError> {
    let project = st
        .services
        .db
        .get_dub_project(&id)
        .await?
        .ok_or_else(|| AppError::not_found("không tìm thấy dự án"))?;
    let path = Path::new(&project.video_path);
    let ct = match path.extension().and_then(|e| e.to_str()) {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    };
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| AppError::not_found("không đọc được file video"))?;
    Ok(([(header::CONTENT_TYPE, ct)], Bytes::from(bytes)).into_response())
}

#[derive(Deserialize)]
struct VideoOffset {
    offset_s: f64,
}

async fn set_video_offset(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(b): Json<VideoOffset>,
) -> Result<Json<Value>, AppError> {
    st.services.db.set_dub_video_offset(&id, b.offset_s).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── Image/banner overlays ───────────────────────────────────────────────────────
fn img_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Upload an image overlay (multipart: `file` + optional geometry fields). The
/// image is stored under `<work_dir>/dub/<project>/overlays/<id>.<ext>`.
async fn create_overlay(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    mut mp: Multipart,
) -> Result<Json<Value>, AppError> {
    if st.services.db.get_dub_project(&id).await?.is_none() {
        return Err(AppError::not_found("không tìm thấy dự án"));
    }
    let mut bytes: Option<Bytes> = None;
    let mut ext = "png".to_string();
    // Geometry (fractions); defaults give a visible top-left banner over the whole clip.
    let (mut start_s, mut end_s, mut x, mut y, mut w, mut opacity) =
        (0.0, 0.0, 0.05, 0.05, 0.3, 1.0);
    let field_f64 = |name: &str, raw: String, slot: &mut f64| {
        if let Ok(v) = raw.trim().parse::<f64>() {
            let _ = name;
            *slot = v;
        }
    };
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("multipart lỗi: {e}")))?
    {
        match field.name().map(|s| s.to_string()).as_deref() {
            Some("file") => {
                if let Some(fname) = field.file_name() {
                    if let Some(e) = Path::new(fname).extension().and_then(|e| e.to_str()) {
                        ext = e.to_ascii_lowercase();
                    }
                }
                bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::bad_request(format!("đọc file: {e}")))?,
                );
            }
            Some("start_s") => field_f64(
                "start_s",
                field.text().await.unwrap_or_default(),
                &mut start_s,
            ),
            Some("end_s") => field_f64("end_s", field.text().await.unwrap_or_default(), &mut end_s),
            Some("x") => field_f64("x", field.text().await.unwrap_or_default(), &mut x),
            Some("y") => field_f64("y", field.text().await.unwrap_or_default(), &mut y),
            Some("w") => field_f64("w", field.text().await.unwrap_or_default(), &mut w),
            Some("opacity") => field_f64(
                "opacity",
                field.text().await.unwrap_or_default(),
                &mut opacity,
            ),
            _ => {}
        }
    }
    let bytes = bytes
        .filter(|b| !b.is_empty())
        .ok_or_else(|| AppError::bad_request("thiếu file ảnh"))?;

    let oid = uuid::Uuid::new_v4().to_string();
    let dir = st.services.work_dir.join("dub").join(&id).join("overlays");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(AppError::internal)?;
    let path = dir.join(format!("{oid}.{ext}"));
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(AppError::internal)?;

    st.services
        .db
        .create_dub_overlay(
            &oid,
            &id,
            &path.to_string_lossy(),
            start_s.max(0.0),
            end_s.max(0.0),
            x.clamp(0.0, 1.0),
            y.clamp(0.0, 1.0),
            w.clamp(0.02, 1.0),
            opacity.clamp(0.0, 1.0),
        )
        .await?;
    let overlay = st.services.db.get_dub_overlay(&oid).await?;
    Ok(Json(json!({ "overlay": overlay })))
}

#[derive(Deserialize)]
struct UpdateOverlay {
    start_s: f64,
    end_s: f64,
    x: f64,
    y: f64,
    w: f64,
    opacity: f64,
}

async fn update_overlay(
    State(st): State<AppState>,
    AxPath(oid): AxPath<String>,
    Json(b): Json<UpdateOverlay>,
) -> Result<Json<Value>, AppError> {
    let found = st
        .services
        .db
        .update_dub_overlay(
            &oid,
            b.start_s.max(0.0),
            b.end_s.max(0.0),
            b.x.clamp(0.0, 1.0),
            b.y.clamp(0.0, 1.0),
            b.w.clamp(0.02, 1.0),
            b.opacity.clamp(0.0, 1.0),
        )
        .await?;
    if !found {
        return Err(AppError::not_found("không tìm thấy overlay"));
    }
    let overlay = st.services.db.get_dub_overlay(&oid).await?;
    Ok(Json(json!({ "overlay": overlay })))
}

async fn delete_overlay(
    State(st): State<AppState>,
    AxPath(oid): AxPath<String>,
) -> Result<Json<Value>, AppError> {
    if let Some(file) = st.services.db.delete_dub_overlay(&oid).await? {
        let _ = tokio::fs::remove_file(&file).await;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn serve_overlay_image(
    State(st): State<AppState>,
    AxPath(oid): AxPath<String>,
) -> Result<Response, AppError> {
    let ov = st
        .services
        .db
        .get_dub_overlay(&oid)
        .await?
        .ok_or_else(|| AppError::not_found("không tìm thấy overlay"))?;
    let path = Path::new(&ov.file);
    let ct = img_content_type(path);
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|_| AppError::not_found("không đọc được ảnh"))?;
    Ok(([(header::CONTENT_TYPE, ct)], Bytes::from(bytes)).into_response())
}
