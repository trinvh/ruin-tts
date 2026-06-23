//! The dubbing step machine. Each function advances one project from one state
//! to the next, writing artifacts under `work_dir/dub/<id>/` and persisting
//! progress so the UI (which polls the project) stays live. Steps are separate
//! so the operator can edit translations / voice mapping in between.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::dub::clients::{GeminiClient, MediaAiClient, TranslateLine};
use crate::dub::{DubSegment, DubSpeaker};
use crate::media;
use crate::nodes::Services;
use crate::tts::{SynthRequest, VoiceInfo};

/// Vietnamese speaking pace (words/sec) used to budget "translate shorter".
const VI_WORDS_PER_SEC: f64 = 2.3;

/// Lines per Gemini call. Long videos (thousands of segments) overflow the
/// model's output-token cap when sent as one request — the response gets clipped
/// mid-JSON ("EOF while parsing a string"). Chunking keeps every response small
/// enough to complete; ~80 short dialogue lines stay well under the cap.
const TRANSLATE_CHUNK: usize = 80;
/// Already-translated lines carried into the next chunk as context so address
/// terms / register stay consistent across the chunk boundary.
const TRANSLATE_CONTEXT: usize = 10;
/// Seconds to wait between chunks ONCE a call has been rate-limited, to stay
/// under the free-tier requests-per-minute ceiling instead of repeatedly firing
/// then eating a 30s 429 backoff. A pro key never trips this (no pacing).
const TRANSLATE_PACE_SECS: u64 = 5;

/// Persist + log one progress beat for a running step. `frac` is 0..1 (or `None`
/// for an indeterminate step); `label` is the Vietnamese description shown in the
/// UI. Also prints to the server console so a stuck run is visible in logs. DB
/// write failures are non-fatal — progress is best-effort telemetry.
async fn report(services: &Services, project_id: &str, frac: Option<f64>, label: &str) {
    match frac {
        Some(f) => tracing::info!("dub[{project_id}] {:>3.0}% — {label}", f * 100.0),
        None => tracing::info!("dub[{project_id}]  ··· — {label}"),
    }
    if let Err(e) = services.db.set_dub_progress(project_id, frac, label).await {
        tracing::warn!("dub[{project_id}] không ghi được progress: {e}");
    }
}

/// Run `fut` while emitting a heartbeat (console log + UI label carrying the
/// elapsed seconds) every 15s, so a slow single-shot step (ffmpeg mux, sidecar
/// analyse) is visibly *alive* instead of looking stuck — e.g. after a UI
/// hot-reload when the operator can't tell if work is still running. Logs the
/// total elapsed on completion.
async fn with_heartbeat<F, T>(services: &Services, project_id: &str, what: &str, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    tokio::pin!(fut);
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
    tick.tick().await; // fires immediately — consume so the first beat lands at +15s
    tracing::info!("dub[{project_id}] {what} — bắt đầu…");
    loop {
        tokio::select! {
            r = &mut fut => {
                tracing::info!(
                    "dub[{project_id}] {what} — xong sau {:.1}s",
                    start.elapsed().as_secs_f64()
                );
                return r;
            }
            _ = tick.tick() => {
                let secs = start.elapsed().as_secs();
                tracing::info!("dub[{project_id}] {what} — vẫn đang chạy ({secs}s)…");
                let _ = services
                    .db
                    .set_dub_progress(project_id, None, &format!("{what}… ({secs}s)"))
                    .await;
            }
        }
    }
}

/// Translate every line in safe-sized chunks, carrying a short rolling context
/// across boundaries (so xưng hô stays consistent over a long script), and
/// self-healing any ids the model drops. Reports per-chunk progress. Returns
/// id → Vietnamese.
async fn translate_all(
    services: &Services,
    project_id: &str,
    client: &GeminiClient,
    lang: &str,
    lines: &[TranslateLine],
) -> Result<HashMap<i64, String>> {
    let mut out: HashMap<i64, String> = HashMap::new();
    // Rolling tail of (source, vi) pairs from the most recent chunk.
    let mut context: Vec<(String, String)> = Vec::new();
    let total = lines.len().div_ceil(TRANSLATE_CHUNK).max(1);
    for (i, chunk) in lines.chunks(TRANSLATE_CHUNK).enumerate() {
        report(
            services,
            project_id,
            Some(i as f64 / total as f64),
            &format!("Dịch lô {}/{} ({} câu)", i + 1, total, chunk.len()),
        )
        .await;
        // Pace only after a 429 has been seen (free-tier key); a pro key stays
        // un-paced and runs at full speed.
        if i > 0 && client.is_throttled() {
            tokio::time::sleep(std::time::Duration::from_secs(TRANSLATE_PACE_SECS)).await;
        }
        let got = translate_chunk(client, lang, chunk, &context).await?;
        for (id, vi) in &got {
            out.insert(*id, vi.clone());
        }
        // Rebuild context from the END of this chunk, in chronological order,
        // skipping any line the model still failed to translate.
        let mut tail: Vec<(String, String)> = Vec::new();
        for l in chunk {
            if let Some(vi) = out.get(&l.id) {
                tail.push((l.text.clone(), vi.clone()));
            }
        }
        let keep = tail.len().saturating_sub(TRANSLATE_CONTEXT);
        context = tail.split_off(keep);
    }
    Ok(out)
}

/// Translate one chunk, then re-request any ids missing from the result —
/// recovering from a clipped response (salvage parsing returns the complete
/// objects; the rest are retried). Splits the request in half when the model
/// returns nothing, to shrink the output. Recursion terminates: a single
/// untranslatable line is returned as-is (its id simply stays unset upstream).
type TranslateFut<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<(i64, String)>>> + Send + 'a>>;

fn translate_chunk<'a>(
    client: &'a GeminiClient,
    lang: &'a str,
    lines: &'a [TranslateLine],
    context: &'a [(String, String)],
) -> TranslateFut<'a> {
    Box::pin(async move {
        let mut got = client.translate(lang, lines, context).await?;
        if lines.len() <= 1 {
            return Ok(got);
        }
        let have: HashSet<i64> = got.iter().map(|(id, _)| *id).collect();
        let missing: Vec<TranslateLine> = lines
            .iter()
            .filter(|l| !have.contains(&l.id))
            .cloned()
            .collect();
        if missing.is_empty() {
            return Ok(got);
        }
        if missing.len() == lines.len() {
            // No progress (likely the whole response was clipped) — halve the
            // request so each side produces a shorter, completable response.
            let mid = lines.len() / 2;
            let mut a = translate_chunk(client, lang, &lines[..mid], context).await?;
            let b = translate_chunk(client, lang, &lines[mid..], context).await?;
            a.extend(b);
            return Ok(a);
        }
        // Partial: retry just the dropped ids (same context).
        let more = translate_chunk(client, lang, &missing, context).await?;
        got.extend(more);
        Ok(got)
    })
}

fn project_dir(services: &Services, project_id: &str) -> PathBuf {
    services.work_dir.join("dub").join(project_id)
}

async fn ensure_dir(dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("tạo thư mục {}", dir.display()))?;
    Ok(())
}

// ── Step 1: extract a 16k mono wav from the source video ──────────────────────
pub async fn extract_audio(services: &Services, project_id: &str) -> Result<()> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    let dir = project_dir(services, project_id);
    ensure_dir(&dir).await?;
    report(services, project_id, None, "Tách âm thanh từ video…").await;
    let audio = dir.join("audio.wav");
    media::run_ffmpeg(&media::extract_audio_args(
        Path::new(&project.video_path),
        &audio,
    ))
    .await
    .context("tách âm thanh từ video")?;
    // Store an ABSOLUTE path: the media-ai sidecar runs from a different working
    // directory, so a relative path (the default work_dir) won't resolve there.
    let audio_abs = std::fs::canonicalize(&audio).unwrap_or(audio);
    services
        .db
        .set_dub_field(project_id, "audio_path", &audio_abs.to_string_lossy())
        .await?;
    Ok(())
}

// ── Step 2: analyse (ASR + diarization + gender) via the media-ai sidecar ─────
pub async fn analyze(services: &Services, project_id: &str) -> Result<()> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    let audio = project
        .audio_path
        .clone()
        .ok_or_else(|| anyhow!("chưa tách âm thanh — chạy bước trích xuất trước"))?;
    // media-ai runs from another cwd → always send an absolute path.
    let audio = std::fs::canonicalize(&audio)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(audio);
    let (base, voice_male, voice_female, global_max_speakers) = {
        let c = services.config.read().await;
        (
            c.media_ai_base.clone(),
            c.dub_voice_male.clone(),
            c.dub_voice_female.clone(),
            c.dub_max_speakers,
        )
    };
    // Cap diarization: project override wins over the global default; a non-
    // positive value means "no cap" (pass None to pyannote).
    let max_speakers = match project.max_speakers {
        Some(n) if n > 0 => Some(n as u32),
        Some(_) => None,
        None if global_max_speakers > 0 => Some(global_max_speakers),
        None => None,
    };
    let client = MediaAiClient::new(base);
    report(
        services,
        project_id,
        None,
        "Nhận dạng lời thoại + tách người nói (có thể mất vài phút)…",
    )
    .await;
    let res = with_heartbeat(
        services,
        project_id,
        "Nhận dạng lời thoại + tách người nói",
        client.analyze(&audio, None, None, max_speakers),
    )
    .await?;

    // Replace garbled overlap segments with the per-speaker transcripts recovered
    // by source separation, so each simultaneous speaker is voiced + subtitled.
    let flat = super::overlap::merge_overlap_segments(&res.segments, &res.overlaps);
    let segs: Vec<DubSegment> = flat
        .iter()
        .enumerate()
        .map(|(i, f)| DubSegment {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            idx: i as i64,
            start_s: f.start,
            end_s: f.end,
            speaker: f.speaker.clone(),
            text_src: f.text_src.clone(),
            text_vi: String::new(),
            voice: None,
            tts_path: None,
            fitted_path: None,
            factor: None,
            status: "pending".into(),
            offset_s: 0.0,
        })
        .collect();
    let mut speakers: Vec<DubSpeaker> = res
        .speakers
        .iter()
        .map(|s| DubSpeaker {
            speaker: s.speaker.clone(),
            gender: s.gender.clone(),
            age: s.age,
            voice: None,
        })
        .collect();
    // Overlap separation can introduce a speaker (e.g. SPEAKER_01) the per-segment
    // diarization didn't surface — add it so it still gets a voice.
    for f in &flat {
        if !speakers.iter().any(|s| s.speaker == f.speaker) {
            speakers.push(DubSpeaker {
                speaker: f.speaker.clone(),
                gender: None,
                age: None,
                voice: None,
            });
        }
    }
    // Auto-map each speaker to a voice matching its detected gender, classifying
    // vieneu voices by the "nam"/"nữ" in their name. Multiple same-gender
    // speakers get DISTINCT voices (round-robin), so a 3-4 person dialogue is
    // voiced by different people. Best-effort: if the voice list can't be
    // fetched, speakers stay unmapped for manual choice.
    let voices = services.tts().await.list_voices().await.unwrap_or_default();
    auto_map_voices(&mut speakers, &voices, &voice_male, &voice_female);

    services.db.replace_dub_segments(project_id, &segs).await?;
    services
        .db
        .replace_dub_speakers(project_id, &speakers)
        .await?;
    services
        .db
        .set_dub_field(project_id, "language", &res.language)
        .await?;
    Ok(())
}

// ── Step 3: translate to Vietnamese with Gemini ───────────────────────────────
pub async fn translate(services: &Services, project_id: &str) -> Result<()> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    let (key, model) = {
        let c = services.config.read().await;
        (c.gemini_api_key.clone(), project.gemini_model.clone())
    };
    if key.trim().is_empty() {
        return Err(anyhow!(
            "chưa cấu hình Gemini API key — đặt trong trang Cài đặt"
        ));
    }
    let segs = services.db.get_dub_segments(project_id).await?;
    if segs.is_empty() {
        return Err(anyhow!("chưa có câu thoại — chạy bước phân tích trước"));
    }
    let lang = project.language.clone().unwrap_or_else(|| "auto".into());
    // First-pass word budget = how many Vietnamese words fit the slot once we
    // allow speeding up to the cap. This makes the very first translation fit the
    // timeline, so a re-translate round is usually unnecessary.
    let lines: Vec<TranslateLine> = segs
        .iter()
        .map(|s| TranslateLine {
            id: s.idx,
            speaker: s.speaker.clone(),
            text: s.text_src.clone(),
            seconds: s.slot(),
            max_words: Some(
                ((s.slot() * VI_WORDS_PER_SEC * project.speed_cap).round() as u32).max(3),
            ),
        })
        .collect();

    let client = GeminiClient::new(key, model);
    let by_idx = translate_all(services, project_id, &client, &lang, &lines).await?;
    for s in &segs {
        if let Some(vi) = by_idx.get(&s.idx) {
            services.db.set_dub_segment_text(&s.id, vi).await?;
        }
    }
    Ok(())
}

// ── Step 4: synthesize each segment with vieneu, then fit to the slot ─────────
pub async fn synthesize(services: &Services, project_id: &str) -> Result<()> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    // Re-run starts clean: drop the old TTS audio so it disappears from the
    // timeline and is rebuilt below (cached segments repopulate instantly).
    services.db.clear_dub_synth(project_id).await?;
    let segs = services.db.get_dub_segments(project_id).await?;
    let speakers = services.db.get_dub_speakers(project_id).await?;
    let voice_by_speaker: HashMap<String, Option<String>> =
        speakers.into_iter().map(|s| (s.speaker, s.voice)).collect();
    let profile = services.profile().await;
    let dir = project_dir(services, project_id);
    ensure_dir(&dir).await?;

    let tts = services.tts().await;
    let ref_cache = std::sync::Mutex::new(HashMap::new());
    // Per-segment progress: count only the lines we'll actually synthesize.
    let total = segs.iter().filter(|s| !s.text_vi.trim().is_empty()).count();
    report(
        services,
        project_id,
        Some(0.0),
        &format!("Chuẩn bị đọc {total} câu…"),
    )
    .await;
    let mut done = 0usize;
    for seg in &segs {
        if seg.text_vi.trim().is_empty() {
            continue;
        }
        let voice = resolve_voice(seg, &voice_by_speaker, &profile.voice);
        synth_one(
            services,
            &tts,
            &profile,
            &dir,
            seg,
            &voice,
            project.speed_cap,
            &ref_cache,
        )
        .await?;
        done += 1;
        report(
            services,
            project_id,
            Some(done as f64 / total.max(1) as f64),
            &format!("Đọc câu {done}/{total}"),
        )
        .await;
    }
    // If the Vietnamese track was already built, rebuild it now so a voice/text
    // change followed by re-synth takes effect in the preview without a manual
    // "Ghép track" step.
    if project.vn_track_path.is_some() {
        build_track(services, project_id).await?;
    }
    Ok(())
}

/// Resolve a dub voice into a `SynthRequest` voice selection. A `clone:<id>`
/// handle is registered with the TTS server (once per run, cached) and used via
/// `ref_id`; a plain preset name passes through as `voice`.
async fn resolve_clone_voice(
    services: &Services,
    tts: &crate::tts::TtsClient,
    voice: &str,
    ref_cache: &std::sync::Mutex<HashMap<String, String>>,
) -> Result<(Option<String>, Option<String>)> {
    let Some(id) = voice.strip_prefix("clone:") else {
        return Ok((Some(voice.to_string()), None));
    };
    if let Some(r) = ref_cache.lock().unwrap().get(id).cloned() {
        return Ok((None, Some(r)));
    }
    let clone = services
        .db
        .get_voice_clone(id)
        .await?
        .ok_or_else(|| anyhow!("giọng nhân bản '{id}' không tồn tại"))?;
    let bytes = tokio::fs::read(&clone.file)
        .await
        .with_context(|| format!("đọc mẫu giọng {}", clone.file))?;
    let ref_id = tts
        .clone_voice(bytes)
        .await
        .context("đăng ký giọng nhân bản với máy chủ TTS")?;
    ref_cache
        .lock()
        .unwrap()
        .insert(id.to_string(), ref_id.clone());
    Ok((None, Some(ref_id)))
}

/// Synthesize + time-fit a single segment (shared by full synth and reshorten).
async fn synth_one(
    services: &Services,
    tts: &crate::tts::TtsClient,
    profile: &crate::config::Profile,
    dir: &Path,
    seg: &DubSegment,
    voice: &str,
    speed_cap: f64,
    ref_cache: &std::sync::Mutex<HashMap<String, String>>,
) -> Result<()> {
    let (voice_sel, ref_id) = resolve_clone_voice(services, tts, voice, ref_cache).await?;
    let req = SynthRequest {
        text: seg.text_vi.clone(),
        voice: voice_sel,
        ref_id,
        emotion: profile.emotion.clone(),
        format: "wav".into(),
        temperature: Some(profile.voice_temperature),
        top_k: Some(profile.voice_top_k),
        top_p: Some(profile.voice_top_p),
        repetition_penalty: Some(profile.voice_repetition_penalty),
        silence_p: Some(0.0),
        paragraph_silence_p: Some(0.0),
    };
    // Cache key uses the stable `voice` label (preset name or `clone:<id>`),
    // never the per-session ref_id.
    let tts_path = tts
        .synth_cached(
            &services.cache_dir,
            &seg.id,
            profile.workflow_version,
            &req,
            voice,
        )
        .await
        .with_context(|| format!("đọc câu {} ({voice})", seg.idx))?;
    let dur = media::probe_duration(&tts_path).await.unwrap_or(0.0);
    let slot = seg.slot();

    // Fit to the slot: only ever speed UP (a too-short clip just leaves a gap).
    let (fitted, factor, status) = if slot <= 0.0 || dur <= slot * 1.02 || dur <= 0.0 {
        (tts_path.clone(), 1.0, "ok")
    } else {
        let raw = dur / slot;
        let (factor, status) = if raw > speed_cap {
            (speed_cap, "long") // flag: too long even at the cap → suggest reshorten
        } else {
            (raw, "ok")
        };
        let out = dir.join(format!("fit_{}.wav", seg.idx));
        media::run_ffmpeg(&media::atempo_args(&tts_path, factor, &out))
            .await
            .with_context(|| format!("co giãn câu {}", seg.idx))?;
        (out, factor, status)
    };

    services
        .db
        .set_dub_segment_synth(
            &seg.id,
            Some(&tts_path.to_string_lossy()),
            Some(&fitted.to_string_lossy()),
            Some(factor),
            status,
        )
        .await?;
    Ok(())
}

/// Re-translate the segments flagged "long" with a word budget that fits their
/// slot, then re-synthesize just those. Lets the operator shorten instead of
/// over-speeding. Returns how many were reshortened.
pub async fn reshorten_long(services: &Services, project_id: &str) -> Result<usize> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    let segs = services.db.get_dub_segments(project_id).await?;
    let long: Vec<&DubSegment> = segs.iter().filter(|s| s.status == "long").collect();
    if long.is_empty() {
        return Ok(0);
    }
    let (key, model) = {
        let c = services.config.read().await;
        (c.gemini_api_key.clone(), project.gemini_model.clone())
    };
    if key.trim().is_empty() {
        return Err(anyhow!("chưa cấu hình Gemini API key"));
    }
    let lang = project.language.clone().unwrap_or_else(|| "auto".into());
    let lines: Vec<TranslateLine> = long
        .iter()
        .map(|s| TranslateLine {
            id: s.idx,
            speaker: s.speaker.clone(),
            text: s.text_src.clone(),
            seconds: s.slot(),
            // Stricter budget for the retry: fit at normal speed (no speed-up).
            max_words: Some(((s.slot() * VI_WORDS_PER_SEC).round() as u32).max(1)),
        })
        .collect();
    let client = GeminiClient::new(key, model);
    let by_idx = translate_all(services, project_id, &client, &lang, &lines).await?;

    let speakers = services.db.get_dub_speakers(project_id).await?;
    let voice_by_speaker: HashMap<String, Option<String>> =
        speakers.into_iter().map(|s| (s.speaker, s.voice)).collect();
    let profile = services.profile().await;
    let dir = project_dir(services, project_id);
    let tts = services.tts().await;
    let ref_cache = std::sync::Mutex::new(HashMap::new());

    let mut count = 0;
    for seg in long {
        if let Some(vi) = by_idx.get(&seg.idx) {
            services.db.set_dub_segment_text(&seg.id, vi).await?;
            let mut updated = seg.clone();
            updated.text_vi = vi.clone();
            let voice = resolve_voice(&updated, &voice_by_speaker, &profile.voice);
            synth_one(
                services,
                &tts,
                &profile,
                &dir,
                &updated,
                &voice,
                project.speed_cap,
                &ref_cache,
            )
            .await?;
            count += 1;
        }
    }
    Ok(count)
}

// ── Step 5: assemble the Vietnamese track on the source timeline ──────────────
pub async fn build_track(services: &Services, project_id: &str) -> Result<()> {
    // Re-run drops the old mixed track until the new one is written below.
    services
        .db
        .clear_dub_field(project_id, "vn_track_path")
        .await?;
    let segs = services.db.get_dub_segments(project_id).await?;
    let dir = project_dir(services, project_id);
    ensure_dir(&dir).await?;
    report(services, project_id, None, "Ghép track tiếng Việt…").await;

    // Place each fitted clip at its *absolute* start time and mix (not serial
    // concat), so the dub tracks the original timeline exactly — overlapping
    // speech overlaps in the output instead of being pushed out / drifting.
    // Collect (fitted clip, absolute start). Length is read from each WAV header
    // inside the mixer — no per-clip ffprobe (2000+ of those alone crawl).
    let mut clips: Vec<(PathBuf, f64)> = Vec::new();
    let mut total = 0.0_f64;
    for seg in &segs {
        let Some(path) = &seg.fitted_path else {
            continue;
        };
        let start = seg.placed_start().max(0.0);
        total = total.max(start); // lower bound; the mixer extends to each clip's true end
        clips.push((PathBuf::from(path), start));
    }
    if clips.is_empty() {
        return Err(anyhow!("chưa có đoạn tiếng Việt nào — chạy bước đọc trước"));
    }
    let out = dir.join("vn_track.wav");
    // Mix by summing PCM samples in Rust (off the async runtime), not a giant
    // ffmpeg `amix` — the latter hangs on long videos with thousands of inputs.
    let n = clips.len();
    let out_path = out.clone();
    let mix =
        tokio::task::spawn_blocking(move || media::mix_clips_to_wav(&clips, total, &out_path));
    with_heartbeat(
        services,
        project_id,
        &format!("Ghép track tiếng Việt ({n} đoạn)"),
        mix,
    )
    .await
    .map_err(|e| anyhow!("tác vụ ghép track lỗi: {e}"))?
    .context("ghép track tiếng Việt")?;
    services
        .db
        .set_dub_field(project_id, "vn_track_path", &out.to_string_lossy())
        .await?;
    Ok(())
}

// ── Step 6: mux the Vietnamese track over the original video ──────────────────
pub async fn export(services: &Services, project_id: &str) -> Result<()> {
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    // Re-run drops the previous export until the new file is written.
    services
        .db
        .clear_dub_field(project_id, "export_path")
        .await?;
    report(
        services,
        project_id,
        None,
        "Dựng & xuất video (encode — có thể mất vài phút)…",
    )
    .await;

    // Clip-driven timeline: refresh the `dub:*` clips, then if any clip exists,
    // render the whole timeline with the generalized compositor and return —
    // bypassing the dub-specific mux path below.
    crate::dub::compose::compose_clips(services, project_id).await?;
    let clips = services.db.list_dub_clips(project_id).await?;
    if !clips.is_empty() {
        export_composited(services, project_id, &clips).await?;
        return Ok(());
    }

    let vn = project
        .vn_track_path
        .clone()
        .ok_or_else(|| anyhow!("chưa dựng track tiếng Việt"))?;
    let dir = project_dir(services, project_id);
    let video = Path::new(&project.video_path);

    // Video track deleted in the editor → export audio only (original + VN mix at
    // their track volumes), no frames or subtitles.
    if !project.video_enabled {
        let out = dir.join("export.m4a");
        with_heartbeat(
            services,
            project_id,
            "Xuất âm thanh tiếng Việt",
            media::run_ffmpeg(&media::export_audio_args(
                video,
                Path::new(&vn),
                &out,
                project.original_volume,
                project.vn_volume,
                project.video_offset_s,
            )),
        )
        .await
        .context("xuất âm thanh tiếng Việt")?;
        services
            .db
            .set_dub_field(
                project_id,
                "export_path",
                &std::fs::canonicalize(&out)
                    .unwrap_or_else(|_| out.clone())
                    .to_string_lossy(),
            )
            .await?;
        return Ok(());
    }

    let out = dir.join("export.mp4");

    // Optionally write the Vietnamese subtitles. Burn them via libass when the
    // ffmpeg build supports it; otherwise embed a soft (selectable) track so
    // subtitles still ship even though they can't be hard-coded.
    let (sub_path, use_burn) = if project.burn_subtitles {
        let segs = services.db.get_dub_segments(project_id).await?;
        // Subtitles shift by the video lead-in too (the burned video is padded).
        let lead = project.video_offset_s;
        let cues: Vec<media::Cue> = segs
            .iter()
            .filter(|s| !s.text_vi.trim().is_empty())
            .map(|s| media::Cue {
                start: s.placed_start() + lead,
                end: s.placed_end() + lead,
                text: &s.text_vi,
                top: if project.sub_bilingual {
                    Some(s.text_src.as_str())
                } else {
                    None
                },
            })
            .collect();
        let srt = media::build_srt(&cues);
        let path = dir.join("subs.srt");
        tokio::fs::write(&path, srt).await.context("ghi phụ đề")?;
        let can_burn = media::has_filter("subtitles").await;
        if !can_burn {
            tracing::warn!(
                "ffmpeg thiếu libass — nhúng phụ đề dạng mềm (mov_text) thay vì ghi cứng vào hình. \
                 Cài ffmpeg có libass nếu muốn phụ đề in cứng."
            );
        }
        (Some(path), can_burn)
    } else {
        (None, false)
    };
    // One probe for both subtitle MarginV and blur feathering (pixel coords).
    let frame = media::probe_video_dimensions(video).await.ok();
    let sub_margin_v = if use_burn {
        frame.map(|(_, h)| (((1.0 - project.sub_y).clamp(0.0, 1.0)) * h as f64).round() as u32)
    } else {
        None
    };

    let overlays = services.db.list_dub_overlays(project_id).await?;
    let overlay_args: Vec<media::OverlayArg> = overlays
        .iter()
        .map(|o| media::OverlayArg {
            path: Path::new(&o.file),
            start: o.start_s,
            end: o.end_s,
            x: o.x,
            y: o.y,
            w: o.w,
            opacity: o.opacity,
        })
        .collect();

    let opts = media::ExportOpts {
        original_volume: project.original_volume,
        vn_volume: project.vn_volume,
        subtitles_burn: if use_burn { sub_path.as_deref() } else { None },
        subtitles_soft: if sub_path.is_some() && !use_burn {
            sub_path.as_deref()
        } else {
            None
        },
        sub_margin_v,
        sub_size: if use_burn {
            Some(project.sub_size)
        } else {
            None
        },
        sub_color: if use_burn {
            Some(project.sub_color.as_str())
        } else {
            None
        },
        blur: if project.blur_subtitle {
            Some((
                project.blur_x,
                project.blur_y,
                project.blur_w,
                project.blur_h,
            ))
        } else {
            None
        },
        frame,
        overlays: overlay_args,
        lead_in: project.video_offset_s,
    };
    with_heartbeat(
        services,
        project_id,
        "Ghép video tiếng Việt (encode)",
        media::run_ffmpeg(&media::export_video_args(
            Path::new(&project.video_path),
            Path::new(&vn),
            &out,
            &opts,
        )),
    )
    .await
    .context("ghép video tiếng Việt")?;
    services
        .db
        .set_dub_field(
            project_id,
            "export_path",
            &std::fs::canonicalize(&out)
                .unwrap_or_else(|_| out.clone())
                .to_string_lossy(),
        )
        .await?;
    Ok(())
}

/// Render the timeline from `dub_clips` with the generalized compositor and set
/// `export_path`. Resolves each clip's source to a path, computes the total
/// timeline length from the clips, and probes the first video clip for the frame
/// size (falling back to 1080p).
async fn export_composited(
    services: &Services,
    project_id: &str,
    clips: &[crate::dub::DubClip],
) -> Result<()> {
    let dir = project_dir(services, project_id);
    ensure_dir(&dir).await?;

    // Total length = furthest clip end on the timeline.
    let total_s = clips
        .iter()
        .map(|c| c.start_s + c.dur_s)
        .fold(0.0_f64, f64::max);

    // Frame size from the first video clip's source (else 1080p).
    let frame = {
        let first_video = clips
            .iter()
            .find(|c| c.kind == "video" && c.source.is_some())
            .and_then(|c| c.source.as_deref());
        match first_video {
            Some(src) => media::probe_video_dimensions(Path::new(src))
                .await
                .unwrap_or((1920, 1080)),
            None => (1920, 1080),
        }
    };

    let kind = |k: &str| match k {
        "video" => media::ClipKind::Video,
        "audio" => media::ClipKind::Audio,
        "image" => media::ClipKind::Image,
        _ => media::ClipKind::Text,
    };
    // Composite in track order (ascending): higher track = on top, drawn last.
    let mut ordered: Vec<&crate::dub::DubClip> = clips.iter().collect();
    ordered.sort_by_key(|c| c.track);

    // Probe each video source once for an audio stream, so its original/background
    // audio is mixed into the export (not just the picture).
    let mut has_audio: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    for c in &ordered {
        if c.kind == "video" {
            if let Some(src) = c.source.as_deref() {
                if !has_audio.contains_key(src) {
                    has_audio.insert(
                        src.to_string(),
                        media::has_audio_stream(Path::new(src)).await,
                    );
                }
            }
        }
    }

    // Subtitles: burn the dub `text` lines with libass (word-wrapping + bundled
    // Vietnamese font, matching the preview) when burning is on and the ffmpeg
    // build has libass. Otherwise drawtext is the fallback; when subtitles are
    // off, drop the text clips so nothing is drawn.
    let project = services
        .db
        .get_dub_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("không tìm thấy dự án"))?;
    let use_ass = project.burn_subtitles && media::has_filter("subtitles").await;
    let drop_text = use_ass || !project.burn_subtitles;

    let clip_args: Vec<media::ClipArg> = ordered
        .iter()
        .filter(|c| !(drop_text && c.kind == "text"))
        .map(|c| media::ClipArg {
            kind: kind(&c.kind),
            source: c.source.as_deref().map(Path::new),
            start: c.start_s,
            dur: c.dur_s,
            in_s: c.in_s,
            volume: c.volume,
            x: c.x,
            y: c.y,
            w: c.w,
            opacity: c.opacity,
            text: c.text.as_deref(),
            audio: c.kind == "video"
                && c.source
                    .as_deref()
                    .and_then(|s| has_audio.get(s).copied())
                    .unwrap_or(false),
        })
        .collect();

    let out = dir.join("export.mp4");

    // Write the ASS subtitle file + the bundled font next to the export.
    let burn = if use_ass {
        let segs = services.db.get_dub_segments(project_id).await?;
        let events: Vec<crate::dub::subtitle::SubEvent> = segs
            .iter()
            .filter(|s| !s.text_vi.trim().is_empty())
            .map(|s| crate::dub::subtitle::SubEvent {
                start_s: s.placed_start(),
                end_s: s.placed_end(),
                vi: &s.text_vi,
                src: Some(s.text_src.as_str()),
            })
            .collect();
        let style = crate::dub::subtitle::SubStyle {
            size: project.sub_size,
            color: &project.sub_color,
            bilingual: project.sub_bilingual,
            bg: project.sub_bg,
        };
        let ass = crate::dub::subtitle::build_ass(&events, &style);
        let ass_path = dir.join("subs.ass");
        tokio::fs::write(&ass_path, ass)
            .await
            .context("ghi file phụ đề ASS")?;
        let fonts_dir = dir.join("fonts");
        ensure_dir(&fonts_dir).await?;
        tokio::fs::write(
            fonts_dir.join("BeVietnamPro-SemiBold.ttf"),
            crate::dub::subtitle::FONT_TTF,
        )
        .await
        .context("ghi font phụ đề")?;
        let ass_abs = std::fs::canonicalize(&ass_path).unwrap_or(ass_path);
        let fonts_abs = std::fs::canonicalize(&fonts_dir).unwrap_or(fonts_dir);
        Some((ass_abs, fonts_abs))
    } else {
        None
    };

    // `drawtext` (the fallback text renderer) needs libfreetype; skip text when
    // the ffmpeg build lacks it so the export still renders the rest.
    let text_ok = media::has_filter("drawtext").await;
    let subtitle_burn = burn.as_ref().map(|(a, f)| media::SubtitleBurn {
        ass: a,
        fonts_dir: f,
    });
    with_heartbeat(
        services,
        project_id,
        &format!("Dựng & xuất video ({} clip)", clip_args.len()),
        media::run_ffmpeg(&media::compose_export_args(
            &clip_args,
            total_s,
            frame,
            &out,
            text_ok,
            subtitle_burn,
        )),
    )
    .await
    .context("dựng timeline (compositing)")?;
    services
        .db
        .set_dub_field(
            project_id,
            "export_path",
            &std::fs::canonicalize(&out)
                .unwrap_or_else(|_| out.clone())
                .to_string_lossy(),
        )
        .await?;
    Ok(())
}

/// Classify a vieneu voice as male/female from the "nam"/"nữ" in its name.
fn classify_voice_gender(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    if n.contains("nữ") || n.contains("female") {
        Some("female")
    } else if n.contains("nam") || n.contains("male") {
        Some("male")
    } else {
        None
    }
}

/// Assign each unmapped speaker a voice matching its gender, round-robin so
/// multiple same-gender speakers get distinct voices. An operator-preferred
/// voice (config) is tried first for its gender. Speakers that already have a
/// voice, or whose gender is unknown / has no matching voice, are left as-is.
fn auto_map_voices(
    speakers: &mut [DubSpeaker],
    voices: &[VoiceInfo],
    prefer_male: &str,
    prefer_female: &str,
) {
    let mut males: Vec<String> = Vec::new();
    let mut females: Vec<String> = Vec::new();
    if !prefer_male.trim().is_empty() {
        males.push(prefer_male.trim().to_string());
    }
    if !prefer_female.trim().is_empty() {
        females.push(prefer_female.trim().to_string());
    }
    for v in voices {
        let bucket = match classify_voice_gender(&format!("{} {}", v.id, v.label)) {
            Some("female") => &mut females,
            Some("male") => &mut males,
            _ => continue,
        };
        if !bucket.contains(&v.id) {
            bucket.push(v.id.clone());
        }
    }

    let (mut mi, mut fi) = (0usize, 0usize);
    for sp in speakers.iter_mut() {
        if sp.voice.as_deref().is_some_and(|v| !v.trim().is_empty()) {
            continue;
        }
        match sp.gender.as_deref() {
            Some("male") if !males.is_empty() => {
                sp.voice = Some(males[mi % males.len()].clone());
                mi += 1;
            }
            Some("female") if !females.is_empty() => {
                sp.voice = Some(females[fi % females.len()].clone());
                fi += 1;
            }
            _ => {}
        }
    }
}

/// Per-segment voice override → speaker's assigned voice → default narration voice.
fn resolve_voice(
    seg: &DubSegment,
    voice_by_speaker: &HashMap<String, Option<String>>,
    default_voice: &str,
) -> String {
    seg.voice
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| voice_by_speaker.get(&seg.speaker).cloned().flatten())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default_voice.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(idx: i64, speaker: &str, voice: Option<&str>) -> DubSegment {
        DubSegment {
            id: format!("s{idx}"),
            project_id: "p".into(),
            idx,
            start_s: 0.0,
            end_s: 1.0,
            speaker: speaker.into(),
            text_src: String::new(),
            text_vi: String::new(),
            voice: voice.map(String::from),
            tts_path: None,
            fitted_path: None,
            factor: None,
            status: "pending".into(),
            offset_s: 0.0,
        }
    }

    fn speaker(name: &str, gender: Option<&str>) -> DubSpeaker {
        DubSpeaker {
            speaker: name.into(),
            gender: gender.map(String::from),
            age: None,
            voice: None,
        }
    }
    fn voice(id: &str, label: &str) -> VoiceInfo {
        VoiceInfo {
            id: id.into(),
            label: label.into(),
        }
    }

    #[test]
    fn auto_map_distributes_voices_by_gender() {
        let voices = vec![
            voice("v1", "Nam Khánh (nam)"),
            voice("v2", "Minh Quân nam"),
            voice("v3", "Ngọc Linh nữ"),
            voice("v4", "Lan Anh (Nữ)"),
            voice("v5", "Robot"), // unclassified → ignored
        ];
        let mut speakers = vec![
            speaker("S0", Some("male")),
            speaker("S1", Some("female")),
            speaker("S2", Some("male")),   // second male → distinct voice
            speaker("S3", Some("female")), // second female → distinct voice
            speaker("S4", None),           // unknown gender → unmapped
        ];
        auto_map_voices(&mut speakers, &voices, "", "");
        assert_eq!(speakers[0].voice.as_deref(), Some("v1"));
        assert_eq!(speakers[1].voice.as_deref(), Some("v3"));
        assert_eq!(speakers[2].voice.as_deref(), Some("v2")); // distinct from S0
        assert_eq!(speakers[3].voice.as_deref(), Some("v4")); // distinct from S1
        assert_eq!(speakers[4].voice, None);
    }

    #[test]
    fn auto_map_prefers_config_voice_and_keeps_existing() {
        let voices = vec![voice("v1", "Một giọng nam")];
        let mut speakers = vec![speaker("S0", Some("male")), speaker("S1", Some("male"))];
        speakers[1].voice = Some("đã chọn".into()); // existing → untouched
        auto_map_voices(&mut speakers, &voices, "ưu tiên nam", "");
        assert_eq!(speakers[0].voice.as_deref(), Some("ưu tiên nam"));
        assert_eq!(speakers[1].voice.as_deref(), Some("đã chọn"));
    }

    #[test]
    fn voice_resolution_precedence() {
        let mut map = HashMap::new();
        map.insert("SPEAKER_00".to_string(), Some("Ngọc Linh".to_string()));
        map.insert("SPEAKER_01".to_string(), None);
        // per-segment override wins
        assert_eq!(
            resolve_voice(&seg(0, "SPEAKER_00", Some("An")), &map, "Default"),
            "An"
        );
        // else speaker voice
        assert_eq!(
            resolve_voice(&seg(1, "SPEAKER_00", None), &map, "Default"),
            "Ngọc Linh"
        );
        // else default
        assert_eq!(
            resolve_voice(&seg(2, "SPEAKER_01", None), &map, "Default"),
            "Default"
        );
    }
}
