//! HTTP endpoints for voice-clone CRUD.
//!
//! WAV samples are stored at `<work_dir>/clones/<id>.wav`.
//! The `GET /api/clones/{id}/sample` endpoint streams the bytes back so the
//! frontend can re-upload them to vieneu-server `/v1/clone` each session.

use std::path::PathBuf;

use axum::{
    body::Bytes,
    extract::{Multipart, Path as AxPath, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::{AppError, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/clones", get(list_clones).post(create_clone))
        .route("/api/clones/{id}", patch(rename_clone).delete(delete_clone))
        .route("/api/clones/{id}/sample", get(serve_sample))
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Absolute path to the WAV file for `id`.
fn clone_path(work_dir: &std::path::Path, id: &str) -> PathBuf {
    work_dir.join("clones").join(format!("{id}.wav"))
}

// ── handlers ──────────────────────────────────────────────────────────────────

/// Public JSON for a clone — the on-disk path is omitted, attribution included.
fn clone_json(c: &crate::db::VoiceClone) -> Value {
    json!({
        "id": c.id,
        "name": c.name,
        "created_at": c.created_at,
        "builtin": c.builtin,
        "source": c.source,
        "license": c.license,
        "source_url": c.source_url,
    })
}

async fn list_clones(State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let clones = st.services.db.list_voice_clones().await?;
    let items: Vec<Value> = clones.iter().map(clone_json).collect();
    Ok(Json(json!({ "clones": items })))
}

async fn create_clone(
    State(st): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let mut name: Option<String> = None;
    let mut wav_bytes: Option<Bytes> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("invalid multipart: {e}")))?
    {
        match field.name() {
            Some("name") => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::bad_request(format!("invalid name field: {e}")))?,
                );
            }
            Some("file") => {
                wav_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::bad_request(format!("invalid file field: {e}")))?,
                );
            }
            _ => {}
        }
    }

    let name = match name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n,
        None => {
            return Err(AppError::bad_request(
                "field 'name' is required and must not be empty",
            ))
        }
    };
    let wav_bytes = match wav_bytes.filter(|b| !b.is_empty()) {
        Some(b) => b,
        None => {
            return Err(AppError::bad_request(
                "field 'file' is required and must not be empty",
            ))
        }
    };

    let id = uuid::Uuid::new_v4().to_string();
    let clones_dir = st.services.work_dir.join("clones");
    tokio::fs::create_dir_all(&clones_dir)
        .await
        .map_err(AppError::internal)?;

    let wav_path = clone_path(&st.services.work_dir, &id);
    tokio::fs::write(&wav_path, &wav_bytes)
        .await
        .map_err(AppError::internal)?;

    let file_str = wav_path.to_string_lossy().to_string();
    st.services
        .db
        .insert_voice_clone(&id, &name, &file_str)
        .await?;

    let clone = st
        .services
        .db
        .get_voice_clone(&id)
        .await?
        .expect("just inserted");

    Ok(Json(clone_json(&clone)))
}

#[derive(Deserialize)]
struct RenameBody {
    name: String,
}

async fn rename_clone(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
    Json(body): Json<RenameBody>,
) -> Result<Json<Value>, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::bad_request("'name' must not be empty"));
    }
    // Built-in voice-pack voices are immutable (attribution must stay intact).
    if let Some(existing) = st.services.db.get_voice_clone(&id).await? {
        if existing.builtin {
            return Err(AppError::bad_request(
                "không thể đổi tên giọng có sẵn trong bộ giọng",
            ));
        }
    }
    let found = st.services.db.rename_voice_clone(&id, &body.name).await?;
    if !found {
        return Err(AppError::not_found("voice clone not found"));
    }
    let clone = st
        .services
        .db
        .get_voice_clone(&id)
        .await?
        .expect("just updated");
    Ok(Json(clone_json(&clone)))
}

async fn delete_clone(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Response, AppError> {
    // Built-in voice-pack voices cannot be deleted.
    if let Some(existing) = st.services.db.get_voice_clone(&id).await? {
        if existing.builtin {
            return Err(AppError::bad_request(
                "không thể xoá giọng có sẵn trong bộ giọng",
            ));
        }
    }
    match st.services.db.delete_voice_clone(&id).await? {
        None => Err(AppError::not_found("voice clone not found")),
        Some(file) => {
            // Best-effort removal of the WAV; ignore "file not found" errors.
            let _ = tokio::fs::remove_file(&file).await;
            Ok((
                axum::http::StatusCode::NO_CONTENT,
                axum::body::Body::empty(),
            )
                .into_response())
        }
    }
}

async fn serve_sample(
    State(st): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Response, AppError> {
    let clone = st
        .services
        .db
        .get_voice_clone(&id)
        .await?
        .ok_or_else(|| AppError::not_found("voice clone not found"))?;
    let bytes = tokio::fs::read(&clone.file)
        .await
        .map_err(|_| AppError::not_found("WAV file not found on disk"))?;
    Ok(([(header::CONTENT_TYPE, "audio/wav")], Bytes::from(bytes)).into_response())
}

// ── tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use crate::db::Db;

    #[tokio::test]
    async fn voice_clone_crud_roundtrip() {
        let db = Db::memory().await.unwrap();

        // Insert
        db.insert_voice_clone("c1", "Linh", "/tmp/c1.wav")
            .await
            .unwrap();

        // List
        let list = db.list_voice_clones().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
        assert_eq!(list[0].name, "Linh");

        // Get
        let c = db.get_voice_clone("c1").await.unwrap().expect("exists");
        assert_eq!(c.file, "/tmp/c1.wav");

        // Rename
        let found = db.rename_voice_clone("c1", "Nam").await.unwrap();
        assert!(found);
        let renamed = db.get_voice_clone("c1").await.unwrap().unwrap();
        assert_eq!(renamed.name, "Nam");

        // Rename non-existent
        assert!(!db.rename_voice_clone("nope", "X").await.unwrap());

        // Delete
        let file = db.delete_voice_clone("c1").await.unwrap();
        assert_eq!(file.as_deref(), Some("/tmp/c1.wav"));
        assert!(db.get_voice_clone("c1").await.unwrap().is_none());

        // Delete non-existent
        assert!(db.delete_voice_clone("c1").await.unwrap().is_none());
    }
}
