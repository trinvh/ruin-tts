//! Per-speaker age + gender via the audeering wav2vec2 ONNX (run with `ort`).
//! Export once with tools/export-agegender-onnx.py; point at it via
//! `--agegender-*` / MEDIA_AI_AGEGENDER_*. When no model is configured (or
//! inference fails) this returns unknown and studio maps voices by name instead.

use crate::onnx::{run_waveform, OrtAny};
use anyhow::{Context, Result};
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct AgeGender {
    pub age: Option<f64>,
    pub gender: Option<String>,
}

pub struct AgeGenderModel {
    // ort's Session::run takes &mut self → Mutex for interior mutability.
    session: Option<Mutex<Session>>,
}

impl AgeGenderModel {
    /// `model_path` = the exported audeering ONNX; `None` disables age/gender.
    pub fn load(model_path: Option<&Path>) -> Result<Self> {
        let session = match model_path {
            Some(p) => {
                let s = Session::builder()
                    .any()?
                    .commit_from_file(p)
                    .any()
                    .with_context(|| format!("nạp model age/gender {}", p.display()))?;
                tracing::info!("age/gender: đã nạp {}", p.display());
                Some(Mutex::new(s))
            }
            None => {
                tracing::warn!(
                    "age/gender: chưa cấu hình model (MEDIA_AI_AGEGENDER_REPO/PATH) — bỏ age/gender"
                );
                None
            }
        };
        Ok(Self { session })
    }

    pub fn enabled(&self) -> bool {
        self.session.is_some()
    }

    /// Predict age + gender for one speaker's (concatenated) audio. Returns
    /// unknown on no-model / empty / inference error (caller degrades).
    pub fn predict(&self, samples: &[f32]) -> AgeGender {
        let Some(mtx) = &self.session else {
            return AgeGender::default();
        };
        if samples.is_empty() {
            return AgeGender::default();
        }
        let res = run_waveform(mtx, samples, |out| {
            let g: Vec<f32> = out["logits_gender"]
                .try_extract_array::<f32>()
                .any()?
                .iter()
                .copied()
                .collect();
            let gender = (g.len() >= 3)
                .then(|| gender_from_logits([g[0], g[1], g[2]]))
                .flatten()
                .map(str::to_string);
            let age = out["logits_age"]
                .try_extract_array::<f32>()
                .any()?
                .iter()
                .next()
                .copied()
                .map(age_from_raw);
            Ok(AgeGender { age, gender })
        });
        match res {
            Ok(ag) => ag,
            Err(e) => {
                tracing::warn!("age/gender inference lỗi: {e}");
                AgeGender::default()
            }
        }
    }
}

/// Gender = argmax of the audeering head logits, ordered `[female, male, child]`.
fn gender_from_logits(logits: [f32; 3]) -> Option<&'static str> {
    const LABELS: [&str; 3] = ["female", "male", "child"];
    let best = (0..3).max_by(|&a, &b| logits[a].total_cmp(&logits[b]))?;
    Some(LABELS[best])
}

/// The age head outputs a value in `[0, 1]`; scale to years, clamp to `[0, 100]`.
fn age_from_raw(raw: f32) -> f64 {
    (raw as f64 * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
