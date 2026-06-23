//! Wire types — identical contract to the Python sidecar's `/analyze`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub audio_path: String,
    #[serde(default)]
    pub hint_lang: Option<String>,
    #[serde(default)]
    pub num_speakers: Option<u32>,
    /// Upper bound on diarization speakers (≠ exact `num_speakers`); caps the
    /// clustering so a long/noisy video can't fragment into phantom speakers.
    #[serde(default)]
    pub max_speakers: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Segment {
    pub id: i64,
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text_src: String,
    pub lang: String,
}

#[derive(Debug, Serialize)]
pub struct Speaker {
    pub speaker: String,
    pub gender: Option<String>,
    pub age: Option<f64>,
}

/// A time span where ≥2 speakers talk at once (pyannote segmentation), with the
/// per-speaker transcripts recovered by source separation (`texts`, one per
/// separated stream) when a separation model is configured.
#[derive(Debug, Serialize)]
pub struct OverlapSpan {
    pub start: f64,
    pub end: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub language: String,
    pub segments: Vec<Segment>,
    pub speakers: Vec<Speaker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender_note: Option<String>,
    /// Overlapping-speech spans (empty if no segmentation model). Additive +
    /// backward-compatible — the Python contract had no such field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps: Vec<OverlapSpan>,
}
