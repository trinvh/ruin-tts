//! Per-speaker age + gender via the audeering wav2vec2 model exported to ONNX
//! (run with `ort`). The model is exported once (see
//! tools/export-agegender-onnx.py) and pointed at via `--agegender-*` /
//! MEDIA_AI_AGEGENDER_*. When no model is configured (or inference fails) this
//! returns unknown and studio maps voices by their names instead.

use anyhow::{Context, Result};
use ndarray::{ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;

/// `ort::Error` isn't `Send + Sync`, so convert via `Display` (cf. vieneu-core).
trait OrtAny<T> {
    fn any(self) -> Result<T>;
}
impl<T, E: std::fmt::Display> OrtAny<T> for std::result::Result<T, E> {
    fn any(self) -> Result<T> {
        self.map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

/// Per-segment features from one wav2vec2 pass: a speaker embedding (the pooled
/// hidden state, for diarization clustering) plus age + gender.
pub struct SegmentFeatures {
    pub embedding: Vec<f32>,
    pub gender: Option<String>,
    pub age: Option<f64>,
}

pub struct Wav2Vec2 {
    // ort's Session::run takes &mut self → Mutex for interior mutability under &self.
    session: Option<Mutex<Session>>,
    /// Name of the embedding output. The exporter can't reliably name a node it
    /// shares with the heads, so we resolve it as "the output that isn't a logit".
    embed_output: String,
}

impl Wav2Vec2 {
    /// `model_path` = the exported audeering ONNX; `None` disables embedding +
    /// age/gender (diarization then falls back to a single speaker).
    pub fn load(model_path: Option<&Path>) -> Result<Self> {
        match model_path {
            Some(p) => {
                let s = Session::builder()
                    .any()?
                    .commit_from_file(p)
                    .any()
                    .with_context(|| format!("nạp model wav2vec2 {}", p.display()))?;
                let embed_output = s
                    .outputs()
                    .iter()
                    .map(|o| o.name().to_string())
                    .find(|n| n != "logits_age" && n != "logits_gender")
                    .unwrap_or_default();
                tracing::info!(
                    "wav2vec2: đã nạp {} (embedding output = {embed_output})",
                    p.display()
                );
                Ok(Self {
                    session: Some(Mutex::new(s)),
                    embed_output,
                })
            }
            None => {
                tracing::warn!(
                    "wav2vec2: chưa cấu hình model (MEDIA_AI_AGEGENDER_REPO/PATH) — bỏ diarization + age/gender"
                );
                Ok(Self {
                    session: None,
                    embed_output: String::new(),
                })
            }
        }
    }

    /// Run the model on one segment's audio → features, or `None` (no model /
    /// empty / inference error — caller degrades gracefully).
    pub fn infer(&self, samples: &[f32]) -> Option<SegmentFeatures> {
        match self.run(samples) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("wav2vec2 inference lỗi: {e}");
                None
            }
        }
    }

    fn run(&self, samples: &[f32]) -> Result<Option<SegmentFeatures>> {
        let Some(mtx) = &self.session else {
            return Ok(None);
        };
        if samples.is_empty() {
            return Ok(None);
        }
        let norm = normalize_waveform(samples);
        let len = norm.len();
        let arr: ArrayD<f32> =
            ArrayD::from_shape_vec(IxDyn(&[1, len]), norm).context("tạo input tensor")?;
        let feeds: Vec<(Cow<'static, str>, SessionInputValue<'static>)> = vec![(
            Cow::Borrowed("input_values"),
            SessionInputValue::from(Tensor::from_array(arr).any()?),
        )];
        let mut session = mtx.lock().map_err(|_| anyhow::anyhow!("wav2vec2 lock"))?;
        let out = session.run(feeds).any()?;

        let emb_val = out
            .get(self.embed_output.as_str())
            .ok_or_else(|| anyhow::anyhow!("thiếu output embedding '{}'", self.embed_output))?;
        let embedding: Vec<f32> = emb_val
            .try_extract_array::<f32>()
            .any()?
            .iter()
            .copied()
            .collect();

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

        Ok(Some(SegmentFeatures {
            embedding,
            gender,
            age,
        }))
    }
}

/// audeering's wav2vec2 feature extractor z-normalizes the raw waveform
/// (zero mean, unit variance). Constant/empty input → all zeros (never NaN).
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
