//! Video dubbing: import a foreign-language video, analyse it (ASR + speaker
//! diarization + gender), translate to Vietnamese with Gemini, synthesize
//! per-segment TTS with vieneu, fit each clip to the source timing with ffmpeg
//! `atempo`, then preview/export the Vietnamese track over the original video.
//!
//! Unlike the audiobook workflow (an unattended node graph), dubbing is
//! human-in-the-loop — the operator edits translations and voice mapping between
//! steps — so it lives in its own tables + endpoints rather than the run engine.

pub mod api;
pub mod clients;
pub mod pipeline;

use serde::{Deserialize, Serialize};

/// A dubbing project: one imported video and its derived artifacts. `status`
/// tracks the step machine: created → extracting → extracted → analyzing →
/// analyzed → translating → translated → synthesizing → synthesized → building →
/// built → exporting → done (and `failed` on error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubProject {
    pub id: String,
    pub name: String,
    pub video_path: String,
    pub audio_path: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub language: Option<String>,
    pub gemini_model: String,
    pub original_volume: f64,
    /// Volume of the Vietnamese dub track (0..1) when muxing the export.
    pub vn_volume: f64,
    pub speed_cap: f64,
    /// Burn the Vietnamese subtitles into the exported video.
    pub burn_subtitles: bool,
    /// Blur a region of the source video to cover hard-coded original subtitles.
    pub blur_subtitle: bool,
    /// Blur rectangle as fractions of the video size (x,y = top-left; w,h = size).
    pub blur_x: f64,
    pub blur_y: f64,
    pub blur_w: f64,
    pub blur_h: f64,
    /// Vertical position of burned subtitles (fraction of height; 0=top, 1=bottom).
    pub sub_y: f64,
    /// Font size (px) of burned subtitles.
    pub sub_size: f64,
    /// Colour of burned subtitles, as a `#RRGGBB` hex string.
    pub sub_color: String,
    /// Render the source text above the Vietnamese (two lines per cue).
    pub sub_bilingual: bool,
    pub vn_track_path: Option<String>,
    pub export_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One transcribed line: source text + Vietnamese translation + synthesis state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubSegment {
    pub id: String,
    pub project_id: String,
    pub idx: i64,
    pub start_s: f64,
    pub end_s: f64,
    pub speaker: String,
    pub text_src: String,
    pub text_vi: String,
    pub voice: Option<String>,
    pub tts_path: Option<String>,
    pub fitted_path: Option<String>,
    pub factor: Option<f64>,
    pub status: String,
}

impl DubSegment {
    pub fn slot(&self) -> f64 {
        (self.end_s - self.start_s).max(0.0)
    }
}

/// A detected speaker, its best-effort gender/age, and the assigned voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DubSpeaker {
    pub speaker: String,
    pub gender: Option<String>,
    pub age: Option<f64>,
    pub voice: Option<String>,
}
