//! Seed the bundled CC-BY voice pack into `voice_clones` on startup.
//!
//! The pack (reference WAVs + `manifest.json`) is embedded in the binary from
//! `crates/studio/assets/voicepack/`. Each entry becomes a `builtin = 1` clone:
//! the WAV is written to `<work_dir>/clones/<id>.wav` (so the existing
//! `/api/clones/{id}/sample` → vieneu `/v1/clone` flow works unchanged) and a
//! row is inserted idempotently. Built-ins are protected from rename/delete and
//! carry the attribution required by CC-BY-4.0.

use std::path::Path;

use anyhow::{Context, Result};
use include_dir::{include_dir, Dir};
use serde::Deserialize;

static VOICE_PACK: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/voicepack");

#[derive(Debug, Deserialize)]
struct PackEntry {
    id: String,
    name: String,
    source: String,
    license: String,
    source_url: String,
}

/// Write each pack WAV to `<work_dir>/clones/` and upsert a built-in row.
/// Errors are non-fatal to startup — a missing/corrupt pack just means no
/// bundled voices, never a crash.
pub async fn seed_builtin_voices(db: &crate::db::Db, work_dir: &Path) -> Result<()> {
    let manifest = VOICE_PACK
        .get_file("manifest.json")
        .context("voicepack manifest.json missing from bundle")?;
    let entries: Vec<PackEntry> =
        serde_json::from_slice(manifest.contents()).context("parse voicepack manifest")?;

    let clones_dir = work_dir.join("clones");
    tokio::fs::create_dir_all(&clones_dir)
        .await
        .context("create clones dir")?;

    let mut seeded = 0usize;
    for e in &entries {
        let wav = match VOICE_PACK.get_file(format!("{}.wav", e.id)) {
            Some(f) => f,
            None => {
                tracing::warn!("voicepack: {}.wav missing from bundle, skipping", e.id);
                continue;
            }
        };
        let wav_path = clones_dir.join(format!("{}.wav", e.id));
        // Restore the WAV if a user deleted it from disk; otherwise leave it.
        if !wav_path.exists() {
            tokio::fs::write(&wav_path, wav.contents())
                .await
                .with_context(|| format!("write {}", wav_path.display()))?;
        }
        let file_str = wav_path.to_string_lossy().to_string();
        db.insert_builtin_voice_clone(
            &e.id,
            &e.name,
            &file_str,
            &e.source,
            &e.license,
            &e.source_url,
        )
        .await?;
        seeded += 1;
    }
    tracing::info!("voicepack: {seeded} built-in voice(s) available");
    Ok(())
}
