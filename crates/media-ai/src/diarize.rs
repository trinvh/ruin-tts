//! Speaker diarization (who-speaks-when).
//!
//! Status: single-speaker fallback covering the whole clip — correct for
//! monologues (most house-tour source videos); multi-speaker clips collapse to
//! one speaker until full diarization lands.
//!
//! TODO(port): the natural crate (`sherpa-rs`) bundles its own ONNX Runtime,
//! which would collide with the `ort` runtime the age/gender model links into
//! the same binary. So real diarization should reuse **ort**: run the pyannote
//! segmentation ONNX over sliding windows, extract speaker embeddings, then
//! cluster (agglomerative) — the clustering step is pure and unit-testable; the
//! ONNX inference + window stitching need on-device validation against pyannote.

use anyhow::Result;

pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

pub struct Diarizer;

impl Diarizer {
    pub fn load() -> Result<Self> {
        Ok(Self)
    }

    pub fn diarize(
        &self,
        _audio: &[f32],
        duration: f64,
        _num_speakers: Option<u32>,
    ) -> Result<Vec<Turn>> {
        tracing::warn!("diarization chưa được port sang Rust — tạm dùng 1 người nói");
        Ok(vec![Turn {
            start: 0.0,
            end: duration,
            speaker: "SPEAKER_00".to_string(),
        }])
    }
}
