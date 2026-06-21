//! Speaker diarization (who-speaks-when).
//!
//! TODO(port): real pyannote diarization via sherpa-onnx (segmentation ONNX +
//! speaker-embedding ONNX + clustering). For now a single-speaker fallback that
//! covers the whole clip — correct for monologues (most house-tour source
//! videos); multi-speaker clips collapse to one speaker until this is ported.

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
