//! Per-speaker age + gender from a voice sample.
//!
//! TODO(port): real audeering wav2vec2 age/gender via ONNX (ort). The model must
//! be exported to ONNX once (see tools/export-agegender-onnx.py) and downloaded
//! on first run. For now returns unknown — studio then maps voices by their
//! names (which already encode nam/nữ), so dubbing still works.

use anyhow::Result;

pub struct AgeGender {
    pub gender: Option<String>,
    pub age: Option<f64>,
}

pub struct AgeGenderModel;

impl AgeGenderModel {
    pub fn load() -> Result<Self> {
        Ok(Self)
    }

    pub fn predict(&self, _samples: &[f32], _sr: u32) -> AgeGender {
        AgeGender {
            gender: None,
            age: None,
        }
    }
}
