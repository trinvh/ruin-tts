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

/// audeering's wav2vec2 feature extractor z-normalizes the raw waveform
/// (zero mean, unit variance). Constant/empty input → all zeros (never NaN).
#[allow(dead_code)] // used by predict() once the ONNX model is wired
fn normalize_waveform(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }
    let n = samples.len() as f32;
    let mean = samples.iter().sum::<f32>() / n;
    let var = samples.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
    let std = var.sqrt();
    if std < 1e-7 {
        return vec![0.0; samples.len()];
    }
    samples.iter().map(|x| (x - mean) / std).collect()
}

/// Gender = argmax of the audeering head logits, ordered `[female, male, child]`.
#[allow(dead_code)]
fn gender_from_logits(logits: [f32; 3]) -> Option<&'static str> {
    const LABELS: [&str; 3] = ["female", "male", "child"];
    let best = (0..3).max_by(|&a, &b| logits[a].total_cmp(&logits[b]))?;
    Some(LABELS[best])
}

/// The age head outputs a value in `[0, 1]`; scale to years, clamp to `[0, 100]`.
#[allow(dead_code)]
fn age_from_raw(raw: f32) -> f64 {
    (raw as f64 * 100.0).clamp(0.0, 100.0)
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
