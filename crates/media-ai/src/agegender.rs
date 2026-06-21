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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_zero_mean_unit_variance() {
        let out = normalize_waveform(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mean: f32 = out.iter().sum::<f32>() / out.len() as f32;
        let var: f32 = out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32;
        assert!(mean.abs() < 1e-4, "mean was {mean}");
        assert!((var - 1.0).abs() < 1e-2, "var was {var}");
    }

    #[test]
    fn normalize_handles_empty_and_constant() {
        assert!(normalize_waveform(&[]).is_empty());
        // constant signal → no variance → all zeros (not NaN).
        let out = normalize_waveform(&[0.5, 0.5, 0.5]);
        assert!(out.iter().all(|x| x.abs() < 1e-3), "got {out:?}");
    }

    #[test]
    fn gender_is_argmax_of_logits() {
        // audeering head order: [female, male, child]
        assert_eq!(gender_from_logits([2.0, 1.0, 0.5]), Some("female"));
        assert_eq!(gender_from_logits([0.1, 3.0, 0.5]), Some("male"));
        assert_eq!(gender_from_logits([0.1, 0.2, 5.0]), Some("child"));
    }

    #[test]
    fn age_scales_to_years_and_clamps() {
        assert!((age_from_raw(0.25) - 25.0).abs() < 1e-6);
        assert!(age_from_raw(-0.1).abs() < 1e-6); // clamp low → 0
        assert!((age_from_raw(1.5) - 100.0).abs() < 1e-6); // clamp high → 100
    }
}
