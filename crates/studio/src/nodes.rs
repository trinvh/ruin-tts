//! Node handlers: the concrete pipeline steps. The graph is intentionally short
//! and readable — five stages from source to upload:
//!
//!   Source → FetchChapters → ChunkNarrate → PostProcess → UploadYouTube
//!
//! `ChunkNarrate` packs whole chapters into duration-bounded chunks and renders
//! one voice file per chunk as `<delay> intro <delay> content <delay> outro
//! <delay>`. `PostProcess` adds the music bed / background and exports the file.
//! Every handler emits `tracing` so a long run is observable, not a black box.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::stream::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Profile;
use crate::db::{Db, OutputRecord};
use crate::idempotency::{content_hash, output_key, OutputKey};
use crate::media::{self, DuckSettings, VoiceDelays};
use crate::packing::{pack_chapters, ChapterMeta, PackConfig, PackResult};
use crate::ruin::{ChapterContent, ChapterParams, Novel, RuinClient};
use crate::template::{make_vars, render, MakeVars, NovelVars, TemplateVars};
use crate::tts::{SynthRequest, TtsClient};
use crate::workflow::{NodeDef, NodeHandler, Registry, RunContext};

/// How many chapters a freshly-dropped Source node covers by default. Keeps a
/// new pipeline bounded — narrating a whole novel is opt-in, never accidental.
const DEFAULT_CHAPTER_WINDOW: u32 = 10;

/// Shared services available to every handler. Config is runtime-editable
/// (from the Settings page), so clients are built per use from the current
/// values.
pub struct Services {
    pub db: Db,
    pub config: tokio::sync::RwLock<crate::config::AppConfig>,
    pub work_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Services {
    pub async fn ruin(&self) -> RuinClient {
        let c = self.config.read().await;
        RuinClient::new(c.ruin_base.clone(), c.ruin_key.clone())
    }
    pub async fn tts(&self) -> TtsClient {
        TtsClient::new(self.config.read().await.tts_base.clone())
    }
    pub async fn profile(&self) -> Profile {
        self.config.read().await.profile.clone()
    }
    pub async fn youtube(&self) -> Option<(crate::youtube::YouTube, String)> {
        let c = self.config.read().await;
        if c.youtube_ready() {
            Some((
                crate::youtube::YouTube::new(
                    c.yt_client_id.clone(),
                    c.yt_client_secret.clone(),
                    c.yt_refresh_token.clone(),
                ),
                c.yt_privacy.clone(),
            ))
        } else {
            None
        }
    }
}

/// Per-chunk artifact, progressively filled as it flows through the graph. One
/// chunk = one packed group of chapters = one output audio/video file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoArtifact {
    pub index: u32,
    pub first: u32,
    pub last: u32,
    pub chapter_numbers: Vec<u32>,
    /// Assembled voice track (intro + content + outro with delays).
    #[serde(default)]
    pub voice_path: Option<String>,
    #[serde(default)]
    pub narration_path: Option<String>,
    #[serde(default)]
    pub intro_path: Option<String>,
    #[serde(default)]
    pub outro_path: Option<String>,
    #[serde(default)]
    pub final_audio_path: Option<String>,
    #[serde(default)]
    pub video_path: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub output_key: Option<String>,
}

fn cfg_str(node: &NodeDef, key: &str) -> Option<String> {
    node.config
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
fn cfg_u32(node: &NodeDef, key: &str) -> Option<u32> {
    node.config
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
}
fn cfg_f64(node: &NodeDef, key: &str) -> Option<f64> {
    node.config.get(key).and_then(|v| v.as_f64())
}
fn cfg_bool(node: &NodeDef, key: &str) -> Option<bool> {
    node.config.get(key).and_then(|v| v.as_bool())
}

fn get_videos(ctx: &RunContext) -> Result<Vec<VideoArtifact>> {
    let v = ctx.get("videos").cloned().unwrap_or(Value::Array(vec![]));
    Ok(serde_json::from_value(v)?)
}
fn set_videos(ctx: &mut RunContext, vids: &[VideoArtifact]) -> Result<()> {
    ctx.set("videos", serde_json::to_value(vids)?);
    Ok(())
}
fn novel(ctx: &RunContext) -> Result<Novel> {
    serde_json::from_value(
        ctx.get("novel")
            .cloned()
            .ok_or_else(|| anyhow!("ctx missing novel"))?,
    )
    .context("decode novel")
}
fn chapters(ctx: &RunContext) -> Result<Vec<ChapterContent>> {
    serde_json::from_value(
        ctx.get("chapters")
            .cloned()
            .ok_or_else(|| anyhow!("ctx missing chapters"))?,
    )
    .context("decode chapters")
}

fn novel_vars(n: &Novel) -> NovelVars {
    NovelVars {
        title: n.title.clone(),
        author: n.author.clone().unwrap_or_default(),
        original_title: n.original_title.clone().unwrap_or_default(),
    }
}

/// Pack chapters into duration-bounded chunks (pure — unit tested).
fn plan_videos(
    chs: &[ChapterContent],
    p: &Profile,
    cap_override: Option<f64>,
) -> (Vec<VideoArtifact>, PackResult) {
    let cfg = PackConfig {
        cap_seconds: cap_override.unwrap_or(p.cap_seconds),
        overhead_seconds: p.overhead_seconds,
        wpm: p.wpm,
    };
    let metas: Vec<ChapterMeta> = chs
        .iter()
        .map(|c| ChapterMeta {
            number: c.number,
            word_count: c.word_count,
            title: Some(c.title.clone()),
            est_seconds: None,
        })
        .collect();
    let result = pack_chapters(&metas, cfg);
    let videos = result
        .videos
        .iter()
        .map(|v| VideoArtifact {
            index: v.index as u32,
            first: v.first,
            last: v.last,
            chapter_numbers: v.chapters.iter().map(|c| c.number).collect(),
            ..Default::default()
        })
        .collect();
    (videos, result)
}

// ── Source: sets the slug + chapter range from config (or queue-injected ctx) ──
pub struct SourceHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for SourceHandler {
    fn node_type(&self) -> &str {
        "Source"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        if ctx.get("slug").is_none() {
            if let Some(slug) = cfg_str(node, "slug") {
                ctx.set("slug", json!(slug));
            }
        }
        let first = ctx
            .get("first")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or_else(|| cfg_u32(node, "first").unwrap_or(1));
        // Bounded default: a fresh pipeline narrates a small window, never the
        // whole novel by accident. The operator widens it explicitly.
        let last = ctx
            .get("last")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or_else(|| {
                cfg_u32(node, "last")
                    .unwrap_or_else(|| first.saturating_add(DEFAULT_CHAPTER_WINDOW - 1))
            });
        ctx.set("first", json!(first));
        ctx.set("last", json!(last));
        let slug = ctx
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        tracing::info!(slug = %slug, first, last, "Source: chapter range resolved");
        ctx.log(format!("Nguồn: {slug} · chương {first}–{last}"));
        Ok(())
    }
}

// ── FetchChapters: pull novel + chapter content for the range from Ruin ────────
pub struct FetchChaptersHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for FetchChaptersHandler {
    fn node_type(&self) -> &str {
        "FetchChapters"
    }
    async fn run(&self, _node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        let slug = ctx
            .get("slug")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("chưa chọn truyện ở khối Nguồn"))?
            .to_string();
        let first = ctx.get("first").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let last = ctx
            .get("last")
            .and_then(|v| v.as_u64())
            .unwrap_or(u32::MAX as u64) as u32;

        let ruin = self.0.ruin().await;
        tracing::info!(slug = %slug, "FetchChapters: loading novel");
        let n = ruin
            .get_novel(&slug)
            .await
            .context("tải thông tin truyện")?;
        ctx.set("novel", serde_json::to_value(&n)?);

        // Page through content, keeping chapters whose number is in [first, last].
        let mut wanted: Vec<ChapterContent> = Vec::new();
        let mut page = 1u32;
        loop {
            tracing::info!(slug = %slug, page, "FetchChapters: page");
            let p = ruin
                .chapters_content(
                    &slug,
                    ChapterParams {
                        page: Some(page),
                        limit: Some(200),
                        order_asc: true,
                    },
                )
                .await
                .with_context(|| format!("tải nội dung chương (trang {page})"))?;
            if p.items.is_empty() {
                break;
            }
            let mut past_end = false;
            for c in p.items {
                if c.number > last {
                    past_end = true;
                } else if c.number >= first {
                    wanted.push(c);
                }
            }
            if past_end || page >= p.meta.total_pages {
                break;
            }
            page += 1;
        }
        wanted.sort_by_key(|c| c.number);
        tracing::info!(slug = %slug, count = wanted.len(), "FetchChapters: done");
        ctx.set("chapters", serde_json::to_value(&wanted)?);
        ctx.log(format!("Đã tải {} chương", wanted.len()));
        Ok(())
    }
}

// ── Chunk: pack chapters into duration-bounded chunks (the `videos` array) ─────
async fn plan_into_ctx(services: &Services, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
    let chs = chapters(ctx)?;
    let p = services.profile().await;
    let cap_override = cfg_u32(node, "cap_seconds").map(|v| v as f64);
    let (videos, pack) = plan_videos(&chs, &p, cap_override);
    if !pack.flagged.is_empty() {
        let flagged: Vec<u32> = pack.flagged.iter().map(|c| c.number).collect();
        tracing::warn!(?flagged, "Chunk: oversize chapters flagged");
        ctx.log(format!(
            "⚠ {} chương vượt giới hạn thời lượng và bị bỏ qua: {:?}",
            flagged.len(),
            flagged
        ));
    }
    ctx.log(format!("Chia thành {} chunk", videos.len()));
    ctx.set("packs", serde_json::to_value(&pack)?);
    set_videos(ctx, &videos)?;
    Ok(())
}

/// TTS a single templated line (intro/outro). `None` when the template (or its
/// rendered text) is empty, so the part is simply skipped.
async fn synth_line(
    services: &Services,
    tts: &TtsClient,
    p: &Profile,
    template: &str,
    vars: &TemplateVars,
    kind: &str,
    index: u32,
) -> Result<Option<PathBuf>> {
    if template.trim().is_empty() {
        return Ok(None);
    }
    let text = render(template, vars)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let req = SynthRequest {
        text,
        voice: Some(p.voice.clone()),
        ref_id: None,
        emotion: p.emotion.clone(),
        format: "wav".into(),
        temperature: Some(p.voice_temperature),
        top_k: Some(p.voice_top_k),
        top_p: Some(p.voice_top_p),
        repetition_penalty: Some(p.voice_repetition_penalty),
        silence_p: Some(p.segment_pause),
        paragraph_silence_p: Some(p.paragraph_pause),
    };
    tracing::info!(chunk = index, kind, "Narrate: speech line");
    let bytes = tts
        .synth(&req)
        .await
        .with_context(|| format!("đọc lời {kind}"))?;
    let out = services.work_dir.join(format!("{kind}_v{index}.wav"));
    tokio::fs::write(&out, &bytes).await?;
    Ok(Some(out))
}

// ── Narrate: render one voice file per chunk currently in `videos` ─────────────
async fn narrate_videos(services: &Services, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
    let chs = chapters(ctx)?;
    let by_num: std::collections::HashMap<u32, &ChapterContent> =
        chs.iter().map(|c| (c.number, c)).collect();
    let p = services.profile().await;
    let n = novel(ctx)?;
    let nv = novel_vars(&n);

    let intro_tpl = cfg_str(node, "intro_template")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| p.intro_template.clone());
    let outro_tpl = cfg_str(node, "outro_template")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| p.outro_template.clone());
    let delays = VoiceDelays {
        before_intro: cfg_f64(node, "delay_before_intro").unwrap_or(p.delay_before_intro),
        after_intro: cfg_f64(node, "delay_after_intro").unwrap_or(p.delay_after_intro),
        after_content: cfg_f64(node, "delay_after_content").unwrap_or(p.delay_after_content),
        after_outro: cfg_f64(node, "delay_after_outro").unwrap_or(p.delay_after_outro),
    };

    let tts = services.tts().await;
    // Narrate up to N chapters at once across the TTS engine pool. Each chapter
    // is still rendered whole by the engine (which does its own sentence-level
    // split + crossfade), so audio is unchanged — only throughput improves.
    let concurrency = cfg_u32(node, "concurrency").unwrap_or(2).max(1) as usize;
    let mut videos = get_videos(ctx)?;
    let total = videos.len();
    tracing::info!(chunks = total, concurrency, "Narrate: rendering");
    for (i, v) in videos.iter_mut().enumerate() {
        let vindex = v.index;
        tracing::info!(
            chunk = vindex,
            progress = format!("{}/{total}", i + 1),
            chapters = ?v.chapter_numbers,
            "Narrate: chunk"
        );
        let content_parts: Vec<PathBuf> = futures::stream::iter(v.chapter_numbers.clone())
            .map(|num| {
                let tts = &tts;
                let by_num = &by_num;
                let p = &p;
                let cache_dir = services.cache_dir.as_path();
                async move {
                    let c = by_num
                        .get(&num)
                        .ok_or_else(|| anyhow!("thiếu nội dung chương {num}"))?;
                    let req = SynthRequest {
                        text: c.content.clone(),
                        voice: Some(p.voice.clone()),
                        ref_id: None,
                        emotion: p.emotion.clone(),
                        format: "wav".into(),
                        temperature: Some(p.voice_temperature),
                        top_k: Some(p.voice_top_k),
                        top_p: Some(p.voice_top_p),
                        repetition_penalty: Some(p.voice_repetition_penalty),
                        silence_p: Some(p.segment_pause),
                        paragraph_silence_p: Some(p.paragraph_pause),
                    };
                    tracing::info!(chunk = vindex, chapter = num, "Narrate: tts chapter");
                    tts.synth_cached(cache_dir, &c.id, p.workflow_version, &req)
                        .await
                        .with_context(|| format!("đọc chương {num}"))
                }
            })
            .buffered(concurrency)
            .try_collect()
            .await?;

        let vars = make_vars(MakeVars {
            novel: nv.clone(),
            first: v.first,
            last: v.last,
            chapter_title: String::new(),
            video_index: v.index,
            site_name: p.site_name.clone(),
        });
        let intro_path =
            synth_line(services, &tts, &p, &intro_tpl, &vars, "intro", v.index).await?;
        let outro_path =
            synth_line(services, &tts, &p, &outro_tpl, &vars, "outro", v.index).await?;

        // Assemble: <delay> intro <delay> content… <delay> outro <delay>.
        let content_refs: Vec<&Path> = content_parts.iter().map(|x| x.as_path()).collect();
        let parts = media::voice_sequence(
            intro_path.as_deref(),
            &content_refs,
            outro_path.as_deref(),
            &delays,
        );
        let voice_out = services.work_dir.join(format!("voice_v{}.wav", v.index));
        media::run_ffmpeg(&media::assemble_args(&parts, &voice_out))
            .await
            .with_context(|| format!("ghép tiếng chunk {}", v.index))?;

        v.voice_path = Some(voice_out.to_string_lossy().into_owned());
        v.intro_path = intro_path.map(|x| x.to_string_lossy().into_owned());
        v.outro_path = outro_path.map(|x| x.to_string_lossy().into_owned());
        ctx.log(format!(
            "Chunk {} ({} chương {}–{}) đã lồng tiếng",
            v.index,
            v.chapter_numbers.len(),
            v.first,
            v.last
        ));
    }
    set_videos(ctx, &videos)?;
    Ok(())
}

/// Chunk only: pack chapters into the `videos` array (use before a Loop).
pub struct ChunkHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for ChunkHandler {
    fn node_type(&self) -> &str {
        "Chunk"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        plan_into_ctx(&self.0, node, ctx).await
    }
}

/// Narrate only: render the chunks already in `videos` (use inside a Loop body).
pub struct NarrateHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for NarrateHandler {
    fn node_type(&self) -> &str {
        "Narrate"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        narrate_videos(&self.0, node, ctx).await
    }
}

/// Chunk + Narrate combined (the simple, non-loop path).
pub struct ChunkNarrateHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for ChunkNarrateHandler {
    fn node_type(&self) -> &str {
        "ChunkNarrate"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        plan_into_ctx(&self.0, node, ctx).await?;
        narrate_videos(&self.0, node, ctx).await
    }
}

// ── PostProcess: music bed + background → exported file, then mark done ────────
pub struct PostProcessHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for PostProcessHandler {
    fn node_type(&self) -> &str {
        "PostProcess"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        let p = self.0.profile().await;
        let n = novel(ctx)?;
        let chs = chapters(ctx)?;
        let by_num: std::collections::HashMap<u32, &ChapterContent> =
            chs.iter().map(|c| (c.number, c)).collect();
        let duck: DuckSettings = (&p.duck).into();
        let make_video = cfg_bool(node, "make_video").unwrap_or(true);
        let bg = cfg_str(node, "background")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| p.background_path.clone());
        let is_video = cfg_bool(node, "background_is_video").unwrap_or(p.background_is_video);

        let mut videos = get_videos(ctx)?;
        let mut max_chapter = 0u32;
        for v in &mut videos {
            let voice = v
                .voice_path
                .clone()
                .or_else(|| v.narration_path.clone())
                .ok_or_else(|| anyhow!("chunk {} chưa được lồng tiếng", v.index))?;
            tracing::info!(chunk = v.index, make_video, "PostProcess: chunk");

            // 1) duck the background music bed under the voice (if configured).
            let body = self.0.work_dir.join(format!("body_v{}.wav", v.index));
            if let Some(bgm) = &p.bg_music_path {
                media::run_ffmpeg(&media::duck_mix_args(
                    voice.as_ref(),
                    bgm.as_ref(),
                    &body,
                    duck,
                ))
                .await
                .context("trộn nhạc nền")?;
            } else {
                tokio::fs::copy(&voice, &body).await?;
            }

            // 2) optional intro music → final exported audio (in target format).
            let final_audio = self
                .0
                .work_dir
                .join(format!("final_v{}.{}", v.index, p.format));
            if let Some(im) = &p.intro_music_path {
                media::run_ffmpeg(&media::prepend_intro_music_args(
                    im.as_ref(),
                    &body,
                    &final_audio,
                ))
                .await
                .context("thêm nhạc mở đầu")?;
            } else {
                media::run_ffmpeg(&media::concat_audio_args(&[body.as_path()], &final_audio))
                    .await
                    .context("xuất audio")?;
            }
            v.final_audio_path = Some(final_audio.to_string_lossy().into_owned());

            // 3) compose video over the background (if enabled + configured).
            if make_video {
                if let Some(bgp) = &bg {
                    let out = self.0.work_dir.join(format!("video_v{}.mp4", v.index));
                    media::run_ffmpeg(&media::compose_video_args(
                        final_audio.as_ref(),
                        bgp.as_ref(),
                        &out,
                        is_video,
                        p.width,
                        p.height,
                    ))
                    .await
                    .context("dựng video")?;
                    v.video_path = Some(out.to_string_lossy().into_owned());
                } else {
                    ctx.log(format!(
                        "Chunk {}: chưa cấu hình ảnh/video nền → chỉ xuất audio",
                        v.index
                    ));
                }
            }

            // 4) idempotency record + per-novel cursor.
            let texts: Vec<&str> = v
                .chapter_numbers
                .iter()
                .filter_map(|num| by_num.get(num).map(|c| c.content.as_str()))
                .collect();
            let hash = content_hash(&texts);
            let key = output_key(&OutputKey {
                novel_slug: &n.slug,
                first: v.first,
                last: v.last,
                workflow_version: p.workflow_version,
                hash: &hash,
            });
            self.0
                .db
                .record_output(&OutputRecord {
                    output_key: key.clone(),
                    novel_slug: n.slug.clone(),
                    first_chapter: v.first as i64,
                    last_chapter: v.last as i64,
                    workflow_version: p.workflow_version as i64,
                    content_hash: hash,
                    status: "rendered".into(),
                })
                .await?;
            v.output_key = Some(key);
            max_chapter = max_chapter.max(v.last);
            ctx.log(format!(
                "Chunk {} xong → {}",
                v.index,
                v.video_path
                    .clone()
                    .or_else(|| v.final_audio_path.clone())
                    .unwrap_or_default()
            ));
        }
        if max_chapter > 0 {
            self.0.db.set_cursor(&n.slug, max_chapter as i64).await.ok();
        }
        set_videos(ctx, &videos)?;
        Ok(())
    }
}

// ── UploadYouTube: render metadata + upload each video (skipped w/o creds) ─────
pub struct UploadYouTubeHandler(pub Arc<Services>);
#[async_trait]
impl NodeHandler for UploadYouTubeHandler {
    fn node_type(&self) -> &str {
        "UploadYouTube"
    }
    async fn run(&self, node: &NodeDef, ctx: &mut RunContext) -> Result<()> {
        let Some((yt, default_privacy)) = self.0.youtube().await else {
            tracing::info!("UploadYouTube: no credentials — skipping");
            ctx.log("Bỏ qua tải lên YouTube (chưa cấu hình thông tin đăng nhập)");
            return Ok(());
        };
        let n = novel(ctx)?;
        let nv = novel_vars(&n);
        let p = self.0.profile().await;
        let privacy = cfg_str(node, "privacy").unwrap_or(default_privacy);
        let mut videos = get_videos(ctx)?;
        for v in &mut videos {
            let path = v
                .video_path
                .clone()
                .ok_or_else(|| anyhow!("chunk {} chưa có video để tải lên", v.index))?;
            let vars = make_vars(MakeVars {
                novel: nv.clone(),
                first: v.first,
                last: v.last,
                chapter_title: String::new(),
                video_index: v.index,
                site_name: p.site_name.clone(),
            });
            let title = render(&p.title_template, &vars)?;
            let description = render(&p.description_template, &vars)?;
            let tags = render(&p.tags_template, &vars)?
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let meta = crate::youtube::VideoMeta {
                title,
                description,
                tags,
                privacy: privacy.clone(),
            };
            tracing::info!(chunk = v.index, "UploadYouTube: uploading");
            let video_id = yt.upload(std::path::Path::new(&path), &meta).await?;
            if let Some(key) = &v.output_key {
                self.0.db.set_output_video(key, &video_id).await.ok();
            }
            ctx.log(format!(
                "Đã tải chunk {} → https://youtu.be/{video_id}",
                v.index
            ));
        }
        set_videos(ctx, &videos)?;
        Ok(())
    }
}

/// Register the built-in handlers against shared services.
pub fn register_default(registry: &mut Registry, services: Arc<Services>) {
    registry.register(Box::new(SourceHandler(services.clone())));
    registry.register(Box::new(FetchChaptersHandler(services.clone())));
    registry.register(Box::new(ChunkNarrateHandler(services.clone())));
    registry.register(Box::new(ChunkHandler(services.clone())));
    registry.register(Box::new(NarrateHandler(services.clone())));
    registry.register(Box::new(PostProcessHandler(services.clone())));
    registry.register(Box::new(UploadYouTubeHandler(services)));
    // Note: `If` and `Loop` are handled directly by the executor, not via a
    // registered handler.
}

/// Palette + config schema for the editor UI: each node type, its label, a short
/// description, and the configurable fields (with input kind + default value so
/// a dropped node is pre-filled and immediately runnable).
pub fn node_specs() -> Value {
    json!([
        {
            "type": "Source", "label": "Nguồn truyện",
            "desc": "Chọn truyện và khoảng chương cần đọc.",
            "fields": [
                { "key": "slug", "label": "Truyện", "kind": "novel", "default": "" },
                { "key": "first", "label": "Từ chương", "kind": "number", "default": 1 },
                { "key": "last", "label": "Đến chương", "kind": "number", "default": DEFAULT_CHAPTER_WINDOW }
            ]
        },
        {
            "type": "FetchChapters", "label": "Lấy chương",
            "desc": "Tải nội dung các chương từ Ruin.",
            "fields": []
        },
        {
            "type": "ChunkNarrate", "label": "Chia theo thời lượng",
            "desc": "Gộp chương thành các chunk theo thời lượng, mỗi chunk = 1 file tiếng: lời mở đầu → nội dung → lời tạm biệt (có khoảng lặng).",
            "fields": [
                { "key": "cap_seconds", "label": "Giới hạn mỗi chunk (giây)", "kind": "number", "default": 5400 },
                { "key": "concurrency", "label": "Số chương đọc song song", "kind": "number", "default": 2 },
                { "key": "intro_template", "label": "Lời mở đầu (để trống = dùng mặc định)", "kind": "textarea", "default": "" },
                { "key": "outro_template", "label": "Lời tạm biệt (để trống = dùng mặc định)", "kind": "textarea", "default": "" },
                { "key": "delay_before_intro", "label": "Lặng trước lời mở đầu (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_intro", "label": "Lặng sau lời mở đầu (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_content", "label": "Lặng trước lời tạm biệt (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_outro", "label": "Lặng cuối (giây)", "kind": "number", "default": 1.2 }
            ]
        },
        {
            "type": "Chunk", "label": "Chia chunk",
            "desc": "Gộp chương thành các chunk theo thời lượng (dùng trước khối Lặp để xử lý từng chunk).",
            "fields": [
                { "key": "cap_seconds", "label": "Giới hạn mỗi chunk (giây)", "kind": "number", "default": 5400 }
            ]
        },
        {
            "type": "Narrate", "label": "Đọc (TTS)",
            "desc": "Lồng tiếng các chunk hiện có: lời mở đầu → nội dung → lời tạm biệt.",
            "fields": [
                { "key": "concurrency", "label": "Số chương đọc song song", "kind": "number", "default": 2 },
                { "key": "intro_template", "label": "Lời mở đầu (trống = mặc định)", "kind": "textarea", "default": "" },
                { "key": "outro_template", "label": "Lời tạm biệt (trống = mặc định)", "kind": "textarea", "default": "" },
                { "key": "delay_before_intro", "label": "Lặng trước lời mở đầu (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_intro", "label": "Lặng sau lời mở đầu (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_content", "label": "Lặng trước lời tạm biệt (giây)", "kind": "number", "default": 0.8 },
                { "key": "delay_after_outro", "label": "Lặng cuối (giây)", "kind": "number", "default": 1.2 }
            ]
        },
        {
            "type": "PostProcess", "label": "Xử lý hậu kỳ",
            "desc": "Trộn nhạc nền, ghép ảnh/video nền, xuất ra file.",
            "fields": [
                { "key": "make_video", "label": "Dựng video (tắt = chỉ xuất audio)", "kind": "bool", "default": true },
                { "key": "background", "label": "Ảnh/Video nền (đường dẫn, trống = mặc định)", "kind": "text", "default": "" },
                { "key": "background_is_video", "label": "Nền là video động", "kind": "bool", "default": false }
            ]
        },
        {
            "type": "UploadYouTube", "label": "Tải lên YouTube (tuỳ chọn)",
            "desc": "Tải từng video lên YouTube với tiêu đề/mô tả từ mẫu.",
            "fields": [
                { "key": "privacy", "label": "Quyền riêng tư", "kind": "select", "default": "private", "options": ["private", "unlisted", "public"] }
            ]
        },
        {
            "type": "Loop", "label": "Lặp (Loop)",
            "desc": "Lặp qua từng mục của một mảng (vd: từng chunk). Nhánh 'body' chạy mỗi vòng, 'done' chạy sau khi xong.",
            "control": true,
            "handles": ["body", "done"],
            "fields": [
                { "key": "over", "label": "Mảng để lặp (khoá ngữ cảnh)", "kind": "text", "default": "videos" }
            ]
        },
        {
            "type": "If", "label": "Điều kiện (If)",
            "desc": "Rẽ nhánh theo điều kiện. Nhánh 'then' nếu đúng, 'else' nếu sai.",
            "control": true,
            "handles": ["then", "else"],
            "fields": [
                { "key": "key", "label": "Khoá ngữ cảnh", "kind": "text", "default": "videos" },
                { "key": "op", "label": "Phép so sánh", "kind": "select", "default": "nonempty", "options": ["nonempty", "empty", "truthy", "eq", "ne", "gt", "lt"] },
                { "key": "value", "label": "Giá trị (cho eq/ne/gt/lt)", "kind": "text", "default": "" }
            ]
        }
    ])
}

/// The default 5-stage pipeline, pre-filled with sensible config so it runs as
/// soon as a novel is picked.
pub fn default_workflow() -> crate::workflow::WorkflowDef {
    use crate::workflow::{EdgeDef, Position, WorkflowDef};
    let chain: [(&str, &str, Value); 5] = [
        (
            "src",
            "Source",
            json!({ "slug": "", "first": 1, "last": DEFAULT_CHAPTER_WINDOW }),
        ),
        ("fetch", "FetchChapters", json!({})),
        (
            "chunk",
            "ChunkNarrate",
            json!({
                "cap_seconds": 5400,
                "concurrency": 2,
                "delay_before_intro": 0.8,
                "delay_after_intro": 0.8,
                "delay_after_content": 0.8,
                "delay_after_outro": 1.2
            }),
        ),
        (
            "post",
            "PostProcess",
            json!({ "make_video": true, "background_is_video": false }),
        ),
        ("upload", "UploadYouTube", json!({ "privacy": "private" })),
    ];
    let nodes = chain
        .iter()
        .enumerate()
        .map(|(i, (id, t, cfg))| NodeDef {
            id: id.to_string(),
            node_type: t.to_string(),
            config: cfg.clone(),
            position: Some(Position {
                x: 80.0 + i as f64 * 240.0,
                y: 120.0,
            }),
        })
        .collect();
    let edges = chain
        .windows(2)
        .map(|w| EdgeDef {
            from: w[0].0.into(),
            to: w[1].0.into(),
            handle: None,
        })
        .collect();
    WorkflowDef {
        id: "default".into(),
        name: "Pipeline mặc định".into(),
        version: 1,
        nodes,
        edges,
    }
}

/// A loop-based pipeline: chunk first, then process each chunk independently in
/// a Loop body (Narrate → PostProcess), so a single failed chunk is its own
/// retriable step. Upload runs after the loop completes.
pub fn loop_workflow() -> crate::workflow::WorkflowDef {
    use crate::workflow::{EdgeDef, Position, WorkflowDef};
    let nd = |id: &str, t: &str, cfg: Value, x: f64, y: f64| NodeDef {
        id: id.to_string(),
        node_type: t.to_string(),
        config: cfg,
        position: Some(Position { x, y }),
    };
    let ed = |from: &str, to: &str, handle: Option<&str>| EdgeDef {
        from: from.to_string(),
        to: to.to_string(),
        handle: handle.map(|s| s.to_string()),
    };
    let nodes = vec![
        nd(
            "src",
            "Source",
            json!({ "slug": "", "first": 1, "last": DEFAULT_CHAPTER_WINDOW }),
            80.0,
            120.0,
        ),
        nd("fetch", "FetchChapters", json!({}), 300.0, 120.0),
        nd(
            "chunk",
            "Chunk",
            json!({ "cap_seconds": 5400 }),
            520.0,
            120.0,
        ),
        nd("loop", "Loop", json!({ "over": "videos" }), 740.0, 120.0),
        nd("narr", "Narrate", json!({ "concurrency": 2 }), 660.0, 300.0),
        nd(
            "post",
            "PostProcess",
            json!({ "make_video": true, "background_is_video": false }),
            900.0,
            300.0,
        ),
        nd(
            "up",
            "UploadYouTube",
            json!({ "privacy": "private" }),
            1000.0,
            120.0,
        ),
    ];
    let edges = vec![
        ed("src", "fetch", None),
        ed("fetch", "chunk", None),
        ed("chunk", "loop", None),
        ed("loop", "narr", Some("body")),
        ed("narr", "post", None),
        ed("post", "loop", None), // back-edge: end of body
        ed("loop", "up", Some("done")),
    ];
    WorkflowDef {
        id: "loop".into(),
        name: "Pipeline có vòng lặp".into(),
        version: 1,
        nodes,
        edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(n: u32, words: u32) -> ChapterContent {
        ChapterContent {
            id: format!("c{n}"),
            number: n,
            volume: None,
            title: format!("T{n}"),
            content: String::new(),
            word_count: words,
        }
    }

    #[test]
    fn plan_videos_packs_chapters_to_budget() {
        let p = Profile {
            cap_seconds: 600.0,
            overhead_seconds: 0.0,
            wpm: 150.0,
            ..Profile::default()
        };
        // four 300-word chapters (120s each) → one 600s chunk
        let chs: Vec<ChapterContent> = (1..=4).map(|n| chapter(n, 300)).collect();
        let (videos, pack) = plan_videos(&chs, &p, None);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].chapter_numbers, vec![1, 2, 3, 4]);
        assert!(pack.flagged.is_empty());
    }

    #[test]
    fn plan_videos_cap_override_splits() {
        let p = Profile {
            cap_seconds: 6000.0,
            overhead_seconds: 0.0,
            wpm: 150.0,
            ..Profile::default()
        };
        let chs: Vec<ChapterContent> = (1..=4).map(|n| chapter(n, 300)).collect();
        // override cap to 300s → 120s chapters pack 2 per chunk
        let (videos, _) = plan_videos(&chs, &p, Some(300.0));
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].chapter_numbers, vec![1, 2]);
        assert_eq!(videos[1].chapter_numbers, vec![3, 4]);
    }

    #[tokio::test]
    async fn source_defaults_to_bounded_window() {
        let services = Arc::new(Services {
            db: Db::memory().await.unwrap(),
            config: tokio::sync::RwLock::new(crate::config::AppConfig::default()),
            work_dir: std::env::temp_dir(),
            cache_dir: std::env::temp_dir(),
        });
        let h = SourceHandler(services);
        let mut ctx = RunContext::default();
        let node = NodeDef {
            id: "src".into(),
            node_type: "Source".into(),
            config: json!({ "slug": "demo", "first": 1 }),
            position: None,
        };
        h.run(&node, &mut ctx).await.unwrap();
        // last must be bounded, not u32::MAX
        let last = ctx.get("last").and_then(|v| v.as_u64()).unwrap();
        assert_eq!(last, DEFAULT_CHAPTER_WINDOW as u64);
    }
}
