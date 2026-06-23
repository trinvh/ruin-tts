//! Tool-owned persistence (SQLite via sqlx). Selection, idempotency/resume,
//! the job queue, workflows, profiles, and assets all live here.

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;

#[derive(Debug, Clone, serde::Serialize)]
pub struct VoiceClone {
    pub id: String,
    pub name: String,
    pub created_at: String,
    /// Absolute path to the WAV file on disk.
    pub file: String,
    /// Bundled voice-pack entry: cannot be renamed/deleted by the user.
    pub builtin: bool,
    /// Attribution (only set for built-in voice-pack voices).
    pub source: Option<String>,
    pub license: Option<String>,
    pub source_url: Option<String>,
}

/// Map a `voice_clones` row (with the builtin/attribution columns) to a struct.
fn row_to_voice_clone(r: sqlx::sqlite::SqliteRow) -> VoiceClone {
    VoiceClone {
        id: r.get("id"),
        name: r.get("name"),
        created_at: r.get("created_at"),
        file: r.get("file"),
        builtin: r.get::<i64, _>("builtin") != 0,
        source: r.get("source"),
        license: r.get("license"),
        source_url: r.get("source_url"),
    }
}

fn row_to_dub_overlay(r: sqlx::sqlite::SqliteRow) -> crate::dub::DubOverlay {
    crate::dub::DubOverlay {
        id: r.get("id"),
        project_id: r.get("project_id"),
        file: r.get("file"),
        start_s: r.get("start_s"),
        end_s: r.get("end_s"),
        x: r.get("x"),
        y: r.get("y"),
        w: r.get("w"),
        opacity: r.get("opacity"),
    }
}

fn row_to_dub_clip(r: sqlx::sqlite::SqliteRow) -> crate::dub::DubClip {
    crate::dub::DubClip {
        id: r.get("id"),
        project_id: r.get("project_id"),
        track: r.get("track"),
        kind: r.get("kind"),
        source: r.get("source"),
        start_s: r.get("start_s"),
        dur_s: r.get("dur_s"),
        in_s: r.get("in_s"),
        volume: r.get("volume"),
        x: r.get("x"),
        y: r.get("y"),
        w: r.get("w"),
        opacity: r.get("opacity"),
        text: r.get("text"),
        text_style: r.get("text_style"),
        origin: r.get("origin"),
    }
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub slug: String,
    pub title: String,
    pub cursor: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct OutputRecord {
    pub output_key: String,
    pub novel_slug: String,
    pub first_chapter: i64,
    pub last_chapter: i64,
    pub workflow_version: i64,
    pub content_hash: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub novel_slug: String,
    pub first_chapter: i64,
    pub last_chapter: i64,
    pub status: String,
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (creating the file if needed) and run migrations.
    pub async fn connect(path: &str) -> Result<Self> {
        let opts =
            SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// In-memory database (single connection so it persists across calls).
    pub async fn memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("run migrations")?;
        Ok(())
    }

    // ── Selections ─────────────────────────────────────────────────────────
    pub async fn upsert_selection(&self, slug: &str, title: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO selections (slug, title) VALUES (?, ?)
             ON CONFLICT(slug) DO UPDATE SET title = excluded.title",
        )
        .bind(slug)
        .bind(title)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_cursor(&self, slug: &str, cursor: i64) -> Result<()> {
        sqlx::query("UPDATE selections SET cursor = ? WHERE slug = ?")
            .bind(cursor)
            .bind(slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn selections(&self) -> Result<Vec<Selection>> {
        let rows = sqlx::query("SELECT slug, title, cursor, enabled FROM selections ORDER BY slug")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| Selection {
                slug: r.get("slug"),
                title: r.get("title"),
                cursor: r.get("cursor"),
                enabled: r.get::<i64, _>("enabled") != 0,
            })
            .collect())
    }

    // ── Outputs / idempotency ────────────────────────────────────────────────
    pub async fn is_output_done(&self, output_key: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM outputs WHERE output_key = ?")
            .bind(output_key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn record_output(&self, rec: &OutputRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO outputs
               (output_key, novel_slug, first_chapter, last_chapter, workflow_version, content_hash, status)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(output_key) DO UPDATE SET status = excluded.status",
        )
        .bind(&rec.output_key)
        .bind(&rec.novel_slug)
        .bind(rec.first_chapter)
        .bind(rec.last_chapter)
        .bind(rec.workflow_version)
        .bind(&rec.content_hash)
        .bind(&rec.status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_output_video(&self, output_key: &str, video_id: &str) -> Result<()> {
        sqlx::query("UPDATE outputs SET status = 'uploaded', video_id = ? WHERE output_key = ?")
            .bind(video_id)
            .bind(output_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Jobs (queue) ─────────────────────────────────────────────────────────
    pub async fn enqueue_job(&self, novel_slug: &str, first: i64, last: i64) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO jobs (id, novel_slug, first_chapter, last_chapter, status) VALUES (?, ?, ?, ?, 'queued')",
        )
        .bind(&id)
        .bind(novel_slug)
        .bind(first)
        .bind(last)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Atomically claim the oldest queued job (marks it running).
    pub async fn claim_next_job(&self) -> Result<Option<Job>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, novel_slug, first_chapter, last_chapter FROM jobs
             WHERE status = 'queued' ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let id: String = row.get("id");
        sqlx::query(
            "UPDATE jobs SET status = 'running', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(Job {
            id,
            novel_slug: row.get("novel_slug"),
            first_chapter: row.get("first_chapter"),
            last_chapter: row.get("last_chapter"),
            status: "running".into(),
        }))
    }

    pub async fn complete_job(&self, id: &str, status: &str, error: Option<&str>) -> Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = ?, error = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── App config (settings) ────────────────────────────────────────────────
    pub async fn load_config_json(&self) -> Result<Option<String>> {
        let row = sqlx::query("SELECT json FROM profiles WHERE name = '__config__'")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("json")))
    }

    pub async fn save_config_json(&self, json: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO profiles (name, json) VALUES ('__config__', ?)
             ON CONFLICT(name) DO UPDATE SET json = excluded.json",
        )
        .bind(json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ── Workflows ────────────────────────────────────────────────────────────
    pub async fn save_workflow(
        &self,
        id: &str,
        name: &str,
        version: i64,
        graph_json: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO workflows (id, name, version, graph) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, version = excluded.version, graph = excluded.graph",
        )
        .bind(id)
        .bind(name)
        .bind(version)
        .bind(graph_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn workflows(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT id, graph FROM workflows ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("id"), r.get("graph")))
            .collect())
    }

    pub async fn get_workflow(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT graph FROM workflows WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("graph")))
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workflows WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Runs + per-node steps ────────────────────────────────────────────────
    /// Create a run and its pending steps. `status` is "queued" or "running".
    /// Create a run row. Steps are NOT pre-created — they're inserted on start
    /// (so loop iterations, whose count is only known at runtime, appear too).
    pub async fn create_run(
        &self,
        id: &str,
        graph_json: &str,
        preview: bool,
        label: &str,
        status: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO runs (id, graph, status, preview, label) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(graph_json)
            .bind(status)
            .bind(preview as i64)
            .bind(label)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Atomically claim the oldest queued run (marks it running).
    pub async fn claim_next_run(&self) -> Result<Option<(String, String, bool)>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT id, graph, preview FROM runs WHERE status = 'queued' ORDER BY created_at LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let id: String = row.get("id");
        sqlx::query(
            "UPDATE runs SET status = 'running', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some((
            id,
            row.get("graph"),
            row.get::<i64, _>("preview") != 0,
        )))
    }

    pub async fn set_run_status(&self, id: &str, status: &str, error: Option<&str>) -> Result<()> {
        sqlx::query(
            "UPDATE runs SET status = ?, error = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a step running, creating it if it doesn't exist yet (steps are
    /// created lazily). `step_id` is the node id, or `node_id#<iter>` for loop
    /// iterations; `idx` is the global execution sequence (for ordering).
    pub async fn step_start(
        &self,
        run_id: &str,
        step_id: &str,
        node_type: &str,
        idx: i64,
        input_json: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO run_steps (run_id, idx, node_id, node_type, status, input_json, started_at)
             VALUES (?, ?, ?, ?, 'running', ?, datetime('now'))
             ON CONFLICT(run_id, node_id) DO UPDATE SET
               status='running', input_json=excluded.input_json,
               started_at=datetime('now'), finished_at=NULL, output_json=NULL",
        )
        .bind(run_id)
        .bind(idx)
        .bind(step_id)
        .bind(node_type)
        .bind(input_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn step_finish(
        &self,
        run_id: &str,
        node_id: &str,
        status: &str,
        output_json: &str,
        ctx_state_json: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE run_steps SET status = ?, output_json = ?, ctx_state = ?, finished_at = datetime('now') WHERE run_id = ? AND node_id = ?",
        )
        .bind(status)
        .bind(output_json)
        .bind(ctx_state_json)
        .bind(run_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Full context snapshot recorded when `node_id` finished (for retry).
    pub async fn step_ctx(&self, run_id: &str, node_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT ctx_state FROM run_steps WHERE run_id = ? AND node_id = ?")
            .bind(run_id)
            .bind(node_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.get::<Option<String>, _>("ctx_state")))
    }

    /// Status of every step in a run, as (node_id, status, finished_at).
    pub async fn step_states(&self, run_id: &str) -> Result<Vec<(String, String, Option<String>)>> {
        let rows =
            sqlx::query("SELECT node_id, status, finished_at FROM run_steps WHERE run_id = ?")
                .bind(run_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("node_id"), r.get("status"), r.get("finished_at")))
            .collect())
    }

    pub async fn run_graph(&self, run_id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT graph FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("graph")))
    }

    /// Mark a run (and its in-flight step) cancelled. Only affects runs that
    /// are still queued/running; terminal runs are left untouched.
    pub async fn cancel_run(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE run_steps SET status='cancelled', finished_at=datetime('now') WHERE run_id=? AND status='running'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "UPDATE runs SET status='cancelled', updated_at=datetime('now') WHERE id=? AND status IN ('queued','running')",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete finished runs (done/failed/cancelled) and their steps.
    pub async fn clear_finished_runs(&self) -> Result<u64> {
        sqlx::query(
            "DELETE FROM run_steps WHERE run_id IN (SELECT id FROM runs WHERE status IN ('done','failed','cancelled'))",
        )
        .execute(&self.pool)
        .await?;
        let res = sqlx::query("DELETE FROM runs WHERE status IN ('done','failed','cancelled')")
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Reset the given steps to pending (used before a retry).
    pub async fn reset_steps(&self, run_id: &str, node_ids: &[String]) -> Result<()> {
        for nid in node_ids {
            sqlx::query(
                "UPDATE run_steps SET status='pending', output_json=NULL, ctx_state=NULL, started_at=NULL, finished_at=NULL WHERE run_id=? AND node_id=?",
            )
            .bind(run_id)
            .bind(nid)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_runs(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT id, status, preview, label, error, created_at, updated_at FROM runs ORDER BY created_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<String, _>("id"),
                    "status": r.get::<String, _>("status"),
                    "preview": r.get::<i64, _>("preview") != 0,
                    "label": r.get::<String, _>("label"),
                    "error": r.get::<Option<String>, _>("error"),
                    "created_at": r.get::<String, _>("created_at"),
                    "updated_at": r.get::<String, _>("updated_at"),
                })
            })
            .collect())
    }

    // ── Video dubbing ────────────────────────────────────────────────────────
    pub async fn create_dub_project(
        &self,
        id: &str,
        name: &str,
        video_path: &str,
        gemini_model: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO dub_projects (id, name, video_path, gemini_model, status, burn_subtitles) VALUES (?, ?, ?, ?, 'created', 1)",
        )
        .bind(id)
        .bind(name)
        .bind(video_path)
        .bind(gemini_model)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_dub_projects(&self) -> Result<Vec<crate::dub::DubProject>> {
        let rows = sqlx::query("SELECT * FROM dub_projects ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(dub_project_from_row).collect())
    }

    pub async fn get_dub_project(&self, id: &str) -> Result<Option<crate::dub::DubProject>> {
        let row = sqlx::query("SELECT * FROM dub_projects WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(dub_project_from_row))
    }

    pub async fn delete_dub_project(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM dub_segments WHERE project_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM dub_speakers WHERE project_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM dub_projects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_dub_status(&self, id: &str, status: &str, error: Option<&str>) -> Result<()> {
        // Clear any prior progress: a new step starts fresh, and a finished step
        // shows no bar.
        sqlx::query(
            "UPDATE dub_projects SET status = ?, error = ?, progress = NULL, progress_label = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persist the in-flight step's progress: `frac` is 0..1 (or `None` for an
    /// indeterminate step) and `label` says what it's doing. Read by the polling
    /// UI to draw a real progress bar.
    pub async fn set_dub_progress(&self, id: &str, frac: Option<f64>, label: &str) -> Result<()> {
        sqlx::query(
            "UPDATE dub_projects SET progress = ?, progress_label = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(frac.map(|f| f.clamp(0.0, 1.0)))
        .bind(label)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_dub_field(&self, id: &str, column: &str, value: &str) -> Result<()> {
        // `column` is from a fixed internal set (never user input) — see callers.
        let sql = format!(
            "UPDATE dub_projects SET {column} = ?, updated_at = datetime('now') WHERE id = ?"
        );
        sqlx::query(&sql)
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Null out a generated-artifact column (whitelisted) — used when a step is
    /// re-run so the stale downstream track drops off the timeline until it is
    /// regenerated.
    pub async fn clear_dub_field(&self, id: &str, column: &str) -> Result<()> {
        let column = match column {
            "vn_track_path" => "vn_track_path",
            "export_path" => "export_path",
            "audio_path" => "audio_path",
            other => anyhow::bail!("clear_dub_field: cột không hợp lệ '{other}'"),
        };
        let sql = format!(
            "UPDATE dub_projects SET {column} = NULL, updated_at = datetime('now') WHERE id = ?"
        );
        sqlx::query(&sql).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    /// Set the video lead-in (seconds of empty space before the video).
    pub async fn set_dub_video_offset(&self, id: &str, offset_s: f64) -> Result<()> {
        sqlx::query(
            "UPDATE dub_projects SET video_offset_s = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(offset_s.max(0.0))
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Reset every segment's synthesized audio (paths + fit factor + status) for a
    /// project, so re-running synthesis starts clean and the TTS tracks clear.
    pub async fn clear_dub_synth(&self, project_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE dub_segments SET tts_path = NULL, fitted_path = NULL, factor = NULL, \
             status = 'pending' WHERE project_id = ?",
        )
        .bind(project_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_dub_settings(
        &self,
        id: &str,
        name: &str,
        gemini_model: &str,
        original_volume: f64,
        vn_volume: f64,
        speed_cap: f64,
        burn_subtitles: bool,
        blur_subtitle: bool,
        blur_rect: (f64, f64, f64, f64),
        sub_y: f64,
        sub_size: f64,
        sub_color: &str,
        sub_bilingual: bool,
        sub_bg: bool,
        video_enabled: bool,
    ) -> Result<()> {
        let (blur_x, blur_y, blur_w, blur_h) = blur_rect;
        sqlx::query(
            "UPDATE dub_projects SET name = ?, gemini_model = ?, original_volume = ?, vn_volume = ?, speed_cap = ?,
               burn_subtitles = ?, blur_subtitle = ?, blur_x = ?, blur_y = ?, blur_w = ?, blur_h = ?,
               sub_y = ?, sub_size = ?, sub_color = ?, sub_bilingual = ?, sub_bg = ?, video_enabled = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(name)
        .bind(gemini_model)
        .bind(original_volume)
        .bind(vn_volume)
        .bind(speed_cap)
        .bind(burn_subtitles as i64)
        .bind(blur_subtitle as i64)
        .bind(blur_x)
        .bind(blur_y)
        .bind(blur_w)
        .bind(blur_h)
        .bind(sub_y)
        .bind(sub_size)
        .bind(sub_color)
        .bind(sub_bilingual as i64)
        .bind(sub_bg as i64)
        .bind(video_enabled as i64)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn replace_dub_segments(
        &self,
        project_id: &str,
        segs: &[crate::dub::DubSegment],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM dub_segments WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for s in segs {
            sqlx::query(
                "INSERT INTO dub_segments (id, project_id, idx, start_s, end_s, speaker, text_src, text_vi, voice, status)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&s.id)
            .bind(project_id)
            .bind(s.idx)
            .bind(s.start_s)
            .bind(s.end_s)
            .bind(&s.speaker)
            .bind(&s.text_src)
            .bind(&s.text_vi)
            .bind(&s.voice)
            .bind(&s.status)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_dub_speakers(
        &self,
        project_id: &str,
        speakers: &[crate::dub::DubSpeaker],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM dub_speakers WHERE project_id = ?")
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for sp in speakers {
            sqlx::query(
                "INSERT INTO dub_speakers (project_id, speaker, gender, age, voice) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(project_id)
            .bind(&sp.speaker)
            .bind(&sp.gender)
            .bind(sp.age)
            .bind(&sp.voice)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_dub_segments(&self, project_id: &str) -> Result<Vec<crate::dub::DubSegment>> {
        let rows = sqlx::query("SELECT * FROM dub_segments WHERE project_id = ? ORDER BY idx")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(dub_segment_from_row).collect())
    }

    pub async fn get_dub_segment(&self, seg_id: &str) -> Result<Option<crate::dub::DubSegment>> {
        let row = sqlx::query("SELECT * FROM dub_segments WHERE id = ?")
            .bind(seg_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(dub_segment_from_row))
    }

    pub async fn get_dub_speakers(&self, project_id: &str) -> Result<Vec<crate::dub::DubSpeaker>> {
        let rows = sqlx::query("SELECT * FROM dub_speakers WHERE project_id = ? ORDER BY speaker")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| crate::dub::DubSpeaker {
                speaker: r.get("speaker"),
                gender: r.get("gender"),
                age: r.get("age"),
                voice: r.get("voice"),
            })
            .collect())
    }

    /// Edit a segment's Vietnamese text and/or per-segment voice (UI override).
    pub async fn update_dub_segment(
        &self,
        seg_id: &str,
        text_vi: &str,
        voice: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE dub_segments SET text_vi = ?, voice = ?, status = 'edited' WHERE id = ?",
        )
        .bind(text_vi)
        .bind(voice)
        .bind(seg_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_dub_segment_text(&self, seg_id: &str, text_vi: &str) -> Result<()> {
        sqlx::query("UPDATE dub_segments SET text_vi = ? WHERE id = ?")
            .bind(text_vi)
            .bind(seg_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Image/banner overlays ─────────────────────────────────────────────────
    pub async fn list_dub_overlays(&self, project_id: &str) -> Result<Vec<crate::dub::DubOverlay>> {
        let rows = sqlx::query(
            "SELECT id, project_id, file, start_s, end_s, x, y, w, opacity \
             FROM dub_overlays WHERE project_id = ? ORDER BY created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_dub_overlay).collect())
    }

    pub async fn get_dub_overlay(&self, id: &str) -> Result<Option<crate::dub::DubOverlay>> {
        let row = sqlx::query(
            "SELECT id, project_id, file, start_s, end_s, x, y, w, opacity \
             FROM dub_overlays WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_dub_overlay))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_dub_overlay(
        &self,
        id: &str,
        project_id: &str,
        file: &str,
        start_s: f64,
        end_s: f64,
        x: f64,
        y: f64,
        w: f64,
        opacity: f64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO dub_overlays (id, project_id, file, start_s, end_s, x, y, w, opacity) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(project_id)
        .bind(file)
        .bind(start_s)
        .bind(end_s)
        .bind(x)
        .bind(y)
        .bind(w)
        .bind(opacity)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update the geometry/timing of an overlay. Returns false if it doesn't exist.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_dub_overlay(
        &self,
        id: &str,
        start_s: f64,
        end_s: f64,
        x: f64,
        y: f64,
        w: f64,
        opacity: f64,
    ) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE dub_overlays SET start_s = ?, end_s = ?, x = ?, y = ?, w = ?, opacity = ? WHERE id = ?",
        )
        .bind(start_s)
        .bind(end_s)
        .bind(x)
        .bind(y)
        .bind(w)
        .bind(opacity)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Delete an overlay, returning its file path (so the caller can remove it).
    pub async fn delete_dub_overlay(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT file FROM dub_overlays WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = row else { return Ok(None) };
        let file: String = r.get("file");
        sqlx::query("DELETE FROM dub_overlays WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(Some(file))
    }

    // ── Timeline clips ─────────────────────────────────────────────────────────
    pub async fn list_dub_clips(&self, project_id: &str) -> Result<Vec<crate::dub::DubClip>> {
        let rows = sqlx::query(
            "SELECT id, project_id, track, kind, source, start_s, dur_s, in_s, volume, \
             x, y, w, opacity, text, text_style, origin \
             FROM dub_clips WHERE project_id = ? ORDER BY track, start_s",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_dub_clip).collect())
    }

    pub async fn get_dub_clip(&self, id: &str) -> Result<Option<crate::dub::DubClip>> {
        let row = sqlx::query(
            "SELECT id, project_id, track, kind, source, start_s, dur_s, in_s, volume, \
             x, y, w, opacity, text, text_style, origin \
             FROM dub_clips WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_dub_clip))
    }

    pub async fn create_dub_clip(&self, c: &crate::dub::DubClip) -> Result<()> {
        sqlx::query(
            "INSERT INTO dub_clips (id, project_id, track, kind, source, start_s, dur_s, in_s, \
             volume, x, y, w, opacity, text, text_style, origin) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&c.id)
        .bind(&c.project_id)
        .bind(c.track)
        .bind(&c.kind)
        .bind(&c.source)
        .bind(c.start_s)
        .bind(c.dur_s)
        .bind(c.in_s)
        .bind(c.volume)
        .bind(c.x)
        .bind(c.y)
        .bind(c.w)
        .bind(c.opacity)
        .bind(&c.text)
        .bind(&c.text_style)
        .bind(&c.origin)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update timing/geometry/props of a clip. Returns false if it doesn't exist.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_dub_clip(
        &self,
        id: &str,
        track: i64,
        start_s: f64,
        dur_s: f64,
        in_s: f64,
        volume: f64,
        x: f64,
        y: f64,
        w: f64,
        opacity: f64,
        text: Option<&str>,
        text_style: Option<&str>,
    ) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE dub_clips SET track = ?, start_s = ?, dur_s = ?, in_s = ?, volume = ?, \
             x = ?, y = ?, w = ?, opacity = ?, text = ?, text_style = ? WHERE id = ?",
        )
        .bind(track)
        .bind(start_s)
        .bind(dur_s)
        .bind(in_s)
        .bind(volume)
        .bind(x)
        .bind(y)
        .bind(w)
        .bind(opacity)
        .bind(text)
        .bind(text_style)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Delete a clip, returning its `source` path (so the caller can remove an
    /// uploaded file). Returns None if the clip doesn't exist.
    pub async fn delete_dub_clip(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT source FROM dub_clips WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = row else { return Ok(None) };
        let source: Option<String> = r.get("source");
        sqlx::query("DELETE FROM dub_clips WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(source)
    }

    /// Delete all clips for a project whose `origin` starts with `prefix` (e.g.
    /// "dub:"). Used by compose to clear regenerated clips without touching
    /// origin='user'.
    pub async fn delete_dub_clips_by_origin_prefix(
        &self,
        project_id: &str,
        prefix: &str,
    ) -> Result<()> {
        sqlx::query("DELETE FROM dub_clips WHERE project_id = ? AND origin LIKE ? || '%'")
            .bind(project_id)
            .bind(prefix)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_dub_segment_synth(
        &self,
        seg_id: &str,
        tts_path: Option<&str>,
        fitted_path: Option<&str>,
        factor: Option<f64>,
        status: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE dub_segments SET tts_path = ?, fitted_path = ?, factor = ?, status = ? WHERE id = ?",
        )
        .bind(tts_path)
        .bind(fitted_path)
        .bind(factor)
        .bind(status)
        .bind(seg_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_dub_speaker_voice(
        &self,
        project_id: &str,
        speaker: &str,
        voice: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE dub_speakers SET voice = ? WHERE project_id = ? AND speaker = ?")
            .bind(voice)
            .bind(project_id)
            .bind(speaker)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Shift a segment on the timeline by `offset_s` seconds (free-move). The
    /// clip duration is unchanged; only its placement and subtitle timing move.
    pub async fn set_dub_segment_offset(&self, seg_id: &str, offset_s: f64) -> Result<()> {
        sqlx::query("UPDATE dub_segments SET offset_s = ? WHERE id = ?")
            .bind(offset_s)
            .bind(seg_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Voice clones ──────────────────────────────────────────────────────────
    pub async fn insert_voice_clone(&self, id: &str, name: &str, file: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO voice_clones (id, name, file, created_at) VALUES (?, ?, ?, datetime('now'))",
        )
        .bind(id)
        .bind(name)
        .bind(file)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Seed a bundled voice-pack voice. Idempotent: existing ids are left as-is
    /// (so a user rename of a built-in — if we ever allow it — is preserved, and
    /// re-seeding on every startup is a no-op).
    pub async fn insert_builtin_voice_clone(
        &self,
        id: &str,
        name: &str,
        file: &str,
        source: &str,
        license: &str,
        source_url: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO voice_clones \
             (id, name, file, created_at, builtin, source, license, source_url) \
             VALUES (?, ?, ?, datetime('now'), 1, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(file)
        .bind(source)
        .bind(license)
        .bind(source_url)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_voice_clones(&self) -> Result<Vec<VoiceClone>> {
        // Built-ins first, then user clones newest-first.
        let rows = sqlx::query(
            "SELECT id, name, created_at, file, builtin, source, license, source_url \
             FROM voice_clones ORDER BY builtin DESC, created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_voice_clone).collect())
    }

    pub async fn get_voice_clone(&self, id: &str) -> Result<Option<VoiceClone>> {
        let row = sqlx::query(
            "SELECT id, name, created_at, file, builtin, source, license, source_url \
             FROM voice_clones WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_voice_clone))
    }

    pub async fn rename_voice_clone(&self, id: &str, name: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE voice_clones SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_voice_clone(&self, id: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT file FROM voice_clones WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        let file: String = r.get("file");
        sqlx::query("DELETE FROM voice_clones WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(Some(file))
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let run = sqlx::query("SELECT id, status, preview, label, error, created_at, updated_at FROM runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = run else { return Ok(None) };
        let steps = sqlx::query(
            "SELECT idx, node_id, node_type, status, input_json, output_json, started_at, finished_at FROM run_steps WHERE run_id = ? ORDER BY idx",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let steps: Vec<serde_json::Value> = steps
            .into_iter()
            .map(|s| {
                let parse = |v: Option<String>| {
                    v.and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                };
                serde_json::json!({
                    "node_id": s.get::<String, _>("node_id"),
                    "node_type": s.get::<String, _>("node_type"),
                    "status": s.get::<String, _>("status"),
                    "input": parse(s.get::<Option<String>, _>("input_json")),
                    "output": parse(s.get::<Option<String>, _>("output_json")),
                    "started_at": s.get::<Option<String>, _>("started_at"),
                    "finished_at": s.get::<Option<String>, _>("finished_at"),
                })
            })
            .collect();
        Ok(Some(serde_json::json!({
            "id": r.get::<String, _>("id"),
            "status": r.get::<String, _>("status"),
            "preview": r.get::<i64, _>("preview") != 0,
            "label": r.get::<String, _>("label"),
            "error": r.get::<Option<String>, _>("error"),
            "created_at": r.get::<String, _>("created_at"),
            "updated_at": r.get::<String, _>("updated_at"),
            "steps": steps,
        })))
    }
}

fn dub_project_from_row(r: sqlx::sqlite::SqliteRow) -> crate::dub::DubProject {
    crate::dub::DubProject {
        id: r.get("id"),
        name: r.get("name"),
        video_path: r.get("video_path"),
        audio_path: r.get("audio_path"),
        status: r.get("status"),
        error: r.get("error"),
        language: r.get("language"),
        gemini_model: r.get("gemini_model"),
        original_volume: r.get("original_volume"),
        vn_volume: r.get("vn_volume"),
        speed_cap: r.get("speed_cap"),
        burn_subtitles: r.get::<i64, _>("burn_subtitles") != 0,
        blur_subtitle: r.get::<i64, _>("blur_subtitle") != 0,
        blur_x: r.get("blur_x"),
        blur_y: r.get("blur_y"),
        blur_w: r.get("blur_w"),
        blur_h: r.get("blur_h"),
        sub_y: r.get("sub_y"),
        sub_size: r.get("sub_size"),
        sub_color: r.get("sub_color"),
        sub_bilingual: r.get::<i64, _>("sub_bilingual") != 0,
        sub_bg: r.get::<i64, _>("sub_bg") != 0,
        video_enabled: r.get::<i64, _>("video_enabled") != 0,
        video_offset_s: r.get("video_offset_s"),
        vn_track_path: r.get("vn_track_path"),
        export_path: r.get("export_path"),
        progress: r.get("progress"),
        progress_label: r.get("progress_label"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn dub_segment_from_row(r: sqlx::sqlite::SqliteRow) -> crate::dub::DubSegment {
    crate::dub::DubSegment {
        id: r.get("id"),
        project_id: r.get("project_id"),
        idx: r.get("idx"),
        start_s: r.get("start_s"),
        end_s: r.get("end_s"),
        speaker: r.get("speaker"),
        text_src: r.get("text_src"),
        text_vi: r.get("text_vi"),
        voice: r.get("voice"),
        tts_path: r.get("tts_path"),
        fitted_path: r.get("fitted_path"),
        factor: r.get("factor"),
        status: r.get("status"),
        offset_s: r.get("offset_s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idempotency_roundtrip() {
        let db = Db::memory().await.unwrap();
        assert!(!db.is_output_done("k1").await.unwrap());
        db.record_output(&OutputRecord {
            output_key: "k1".into(),
            novel_slug: "s".into(),
            first_chapter: 1,
            last_chapter: 5,
            workflow_version: 1,
            content_hash: "h".into(),
            status: "rendered".into(),
        })
        .await
        .unwrap();
        assert!(db.is_output_done("k1").await.unwrap());
        db.set_output_video("k1", "yt123").await.unwrap();
    }

    #[tokio::test]
    async fn selection_cursor() {
        let db = Db::memory().await.unwrap();
        db.upsert_selection("s", "Title").await.unwrap();
        db.upsert_selection("s", "Title 2").await.unwrap(); // upsert
        db.set_cursor("s", 12).await.unwrap();
        let sels = db.selections().await.unwrap();
        assert_eq!(sels.len(), 1);
        assert_eq!(sels[0].title, "Title 2");
        assert_eq!(sels[0].cursor, 12);
    }

    #[tokio::test]
    async fn dub_project_roundtrip() {
        let db = Db::memory().await.unwrap();
        db.create_dub_project("p1", "Phim", "/tmp/v.mp4", "gemini-2.5-flash")
            .await
            .unwrap();
        // row mappers read every column by name — this would panic on a typo.
        let p = db.get_dub_project("p1").await.unwrap().expect("project");
        assert_eq!(p.name, "Phim");
        assert_eq!(p.status, "created");
        assert_eq!(p.original_volume, 0.15);

        let segs = vec![crate::dub::DubSegment {
            id: "s1".into(),
            project_id: "p1".into(),
            idx: 0,
            start_s: 0.0,
            end_s: 2.0,
            speaker: "SPEAKER_00".into(),
            text_src: "你好".into(),
            text_vi: String::new(),
            voice: None,
            tts_path: None,
            fitted_path: None,
            factor: None,
            status: "pending".into(),
            offset_s: 0.0,
        }];
        db.replace_dub_segments("p1", &segs).await.unwrap();
        let speakers = vec![crate::dub::DubSpeaker {
            speaker: "SPEAKER_00".into(),
            gender: Some("female".into()),
            age: Some(30.0),
            voice: None,
        }];
        db.replace_dub_speakers("p1", &speakers).await.unwrap();

        db.set_dub_segment_text("s1", "Xin chào").await.unwrap();
        let got = db.get_dub_segments("p1").await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text_vi, "Xin chào");
        db.set_dub_speaker_voice("p1", "SPEAKER_00", Some("Ngọc Linh"))
            .await
            .unwrap();
        let sp = db.get_dub_speakers("p1").await.unwrap();
        assert_eq!(sp[0].voice.as_deref(), Some("Ngọc Linh"));

        db.set_dub_status("p1", "analyzed", None).await.unwrap();
        assert_eq!(
            db.get_dub_project("p1").await.unwrap().unwrap().status,
            "analyzed"
        );
        db.delete_dub_project("p1").await.unwrap();
        assert!(db.get_dub_project("p1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn job_queue_claim_complete() {
        let db = Db::memory().await.unwrap();
        let id = db.enqueue_job("s", 1, 5).await.unwrap();
        let job = db.claim_next_job().await.unwrap().expect("a job");
        assert_eq!(job.id, id);
        assert_eq!(job.status, "running");
        assert!(db.claim_next_job().await.unwrap().is_none()); // no more queued
        db.complete_job(&id, "done", None).await.unwrap();
    }
}
