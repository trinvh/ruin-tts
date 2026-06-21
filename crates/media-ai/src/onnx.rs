//! Shared `ort` helpers for the raw-waveform models (wav2vec2 age/gender, WavLM
//! speaker embedding): error conversion, z-normalization, single-input run.

use anyhow::{anyhow, Context, Result};
use ndarray::{ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue, SessionOutputs};
use ort::value::Tensor;
use std::borrow::Cow;
use std::sync::Mutex;

/// `ort::Error` isn't `Send + Sync`, so convert via `Display`.
pub trait OrtAny<T> {
    fn any(self) -> Result<T>;
}
impl<T, E: std::fmt::Display> OrtAny<T> for std::result::Result<T, E> {
    fn any(self) -> Result<T> {
        self.map_err(|e| anyhow!(e.to_string()))
    }
}

/// z-normalize the raw waveform (zero mean, unit variance); empty/constant → zeros.
pub fn normalize_waveform(samples: &[f32]) -> Vec<f32> {
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

/// Run a single-input (`input_values` = the z-normalized waveform) model; the
/// closure pulls owned data out of the outputs while the session is locked.
pub fn run_waveform<T>(
    mtx: &Mutex<Session>,
    samples: &[f32],
    extract: impl FnOnce(&SessionOutputs) -> Result<T>,
) -> Result<T> {
    let norm = normalize_waveform(samples);
    let arr: ArrayD<f32> =
        ArrayD::from_shape_vec(IxDyn(&[1, norm.len()]), norm).context("input tensor")?;
    let feeds: Vec<(Cow<'static, str>, SessionInputValue<'static>)> = vec![(
        Cow::Borrowed("input_values"),
        SessionInputValue::from(Tensor::from_array(arr).any()?),
    )];
    let mut session = mtx.lock().map_err(|_| anyhow!("onnx lock"))?;
    let out = session.run(feeds).any()?;
    extract(&out)
}

/// The model's output name not in `known` — the exporter can't always name a
/// node shared with other heads, so we resolve it from metadata.
pub fn output_name_excluding(session: &Session, known: &[&str]) -> String {
    session
        .outputs()
        .iter()
        .map(|o| o.name().to_string())
        .find(|n| !known.contains(&n.as_str()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_to_zero_mean_unit_variance() {
        let out = normalize_waveform(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let mean: f32 = out.iter().sum::<f32>() / out.len() as f32;
        let var: f32 = out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32;
        assert!(mean.abs() < 1e-4);
        assert!((var - 1.0).abs() < 1e-2);
    }

    #[test]
    fn normalize_empty_and_constant() {
        assert!(normalize_waveform(&[]).is_empty());
        assert!(normalize_waveform(&[0.5, 0.5, 0.5])
            .iter()
            .all(|x| x.abs() < 1e-3));
    }
}
