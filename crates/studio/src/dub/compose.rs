//! Compose step: regenerate the `dub:*` timeline clips from the current dub data
//! (source video + per-segment TTS + subtitles + image banners). User-added
//! clips (`origin='user'`) are left untouched. Idempotent by `origin`: callers
//! may re-run it after synth/build to refresh the timeline.

use std::path::Path;

use crate::dub::DubClip;
use crate::nodes::Services;

/// Build a `DubClip` with the geometry/timing defaults of a non-visual clip,
/// then let the caller override the relevant fields. Keeps the per-kind builders
/// below short and consistent.
fn clip(
    project_id: &str,
    track: i64,
    kind: &str,
    origin: String,
    start_s: f64,
    dur_s: f64,
) -> DubClip {
    DubClip {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        track,
        kind: kind.to_string(),
        source: None,
        start_s,
        dur_s,
        in_s: 0.0,
        volume: 1.0,
        x: 0.0,
        y: 0.0,
        w: 1.0,
        opacity: 1.0,
        text: None,
        text_style: None,
        origin,
    }
}

/// Regenerate every `dub:*` clip for `project_id` from the current dub state.
pub async fn compose_clips(services: &Services, project_id: &str) -> anyhow::Result<()> {
    let db = &services.db;

    // Clear previously generated dub clips (origin='user' clips are preserved).
    db.delete_dub_clips_by_origin_prefix(project_id, "dub:")
        .await?;

    let project = db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("không tìm thấy dự án: {project_id}"))?;
    let segments = db.get_dub_segments(project_id).await?;
    let overlays = db.list_dub_overlays(project_id).await?;

    // Total timeline duration: probe the source video, falling back to the
    // furthest segment's placed end (so banners shown "for the whole video"
    // still get a sensible length even when probing fails).
    let furthest_end = segments
        .iter()
        .map(|s| s.placed_end())
        .fold(0.0_f64, f64::max);
    let video_dur = crate::media::probe_duration(Path::new(&project.video_path))
        .await
        .unwrap_or(0.0)
        .max(furthest_end);

    // Track 0 — source video (carries the original audio via its volume).
    let mut video = clip(
        project_id,
        0,
        "video",
        "dub:video".to_string(),
        project.video_offset_s,
        video_dur,
    );
    video.source = Some(project.video_path.clone());
    video.volume = project.original_volume;
    db.create_dub_clip(&video).await?;

    for seg in &segments {
        let has_text = !seg.text_vi.trim().is_empty();

        // Track 1 — synthesized Vietnamese audio (only when a fitted clip exists).
        if has_text {
            if let Some(fitted) = seg.fitted_path.as_deref() {
                let mut a = clip(
                    project_id,
                    1,
                    "audio",
                    format!("dub:tts:{}", seg.id),
                    seg.placed_start(),
                    seg.slot(),
                );
                a.source = Some(fitted.to_string());
                db.create_dub_clip(&a).await?;
            }

            // Track 2 — subtitle text.
            let mut t = clip(
                project_id,
                2,
                "text",
                format!("dub:sub:{}", seg.id),
                seg.placed_start(),
                seg.slot(),
            );
            t.text = Some(seg.text_vi.clone());
            db.create_dub_clip(&t).await?;
        }
    }

    // Track 3 — image banners.
    for ov in &overlays {
        let dur = if ov.end_s > ov.start_s {
            ov.end_s - ov.start_s
        } else {
            (video_dur - ov.start_s).max(0.0)
        };
        let mut img = clip(
            project_id,
            3,
            "image",
            format!("dub:banner:{}", ov.id),
            ov.start_s,
            dur,
        );
        img.source = Some(ov.file.clone());
        img.x = ov.x;
        img.y = ov.y;
        img.w = ov.w;
        img.opacity = ov.opacity;
        db.create_dub_clip(&img).await?;
    }

    Ok(())
}
