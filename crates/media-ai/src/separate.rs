//! Speech separation for overlapping regions via a 2-speaker ConvTasNet ONNX
//! (asteroid Libri2Mix-16k). Splits a mixed waveform `mix` [1,n] into `sources`
//! [1,n_src,n] so each simultaneous speaker can be transcribed separately.
//! Export with tools/export-separation-onnx.py; MEDIA_AI_SEPARATE_*.

use crate::onnx::OrtAny;
use anyhow::{Context, Result};
use ndarray::{ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;

pub struct Separator {
    session: Option<Mutex<Session>>,
}

impl Separator {
    /// `model_path` = the ConvTasNet ONNX; `None` disables separation (overlap
    /// regions keep their single mixed transcript).
    pub fn load(model_path: Option<&Path>) -> Result<Self> {
        let session = match model_path {
            Some(p) => {
                let s = Session::builder()
                    .any()?
                    .commit_from_file(p)
                    .any()
                    .with_context(|| format!("nạp model separation {}", p.display()))?;
                tracing::info!("separation: đã nạp {}", p.display());
                Some(Mutex::new(s))
            }
            None => {
                tracing::warn!(
                    "separation: chưa cấu hình model (MEDIA_AI_SEPARATE_REPO/PATH) — bỏ tách giọng chồng"
                );
                None
            }
        };
        Ok(Self { session })
    }

    /// Split mixed audio into per-source streams. Empty when no model / on error.
    pub fn separate(&self, samples: &[f32]) -> Vec<Vec<f32>> {
        let Some(mtx) = &self.session else {
            return Vec::new();
        };
        if samples.is_empty() {
            return Vec::new();
        }
        match self.run(mtx, samples) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("separation lỗi: {e}");
                Vec::new()
            }
        }
    }

    fn run(&self, mtx: &Mutex<Session>, samples: &[f32]) -> Result<Vec<Vec<f32>>> {
        let arr: ArrayD<f32> = ArrayD::from_shape_vec(IxDyn(&[1, samples.len()]), samples.to_vec())
            .context("separation input tensor")?;
        let feeds: Vec<(Cow<'static, str>, SessionInputValue<'static>)> = vec![(
            Cow::Borrowed("mix"),
            SessionInputValue::from(Tensor::from_array(arr).any()?),
        )];
        let mut session = mtx.lock().map_err(|_| anyhow::anyhow!("separation lock"))?;
        let out = session.run(feeds).any()?;
        let y = out["sources"].try_extract_array::<f32>().any()?;
        let shape = y.shape().to_vec(); // [1, n_src, n]
        let (n_src, n) = (shape[1], shape[2]);
        let flat: Vec<f32> = y.iter().copied().collect();
        // ConvTasNet sources are un-normalized (peak ≫ 1) — scale each to ~[-1,1]
        // so whisper (which expects [-1,1]) doesn't read them as noise.
        Ok(split_sources(&flat, n_src, n)
            .into_iter()
            .map(peak_normalize)
            .collect())
    }
}

/// Split a flat `[n_src * n]` (row-major, batch 1) into `n_src` streams.
pub fn split_sources(flat: &[f32], n_src: usize, n: usize) -> Vec<Vec<f32>> {
    (0..n_src)
        .map(|i| flat[i * n..(i + 1) * n].to_vec())
        .collect()
}

/// Scale a stream so its peak is ~0.95 (headroom). Silence passes through.
pub fn peak_normalize(mut s: Vec<f32>) -> Vec<f32> {
    let peak = s.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    if peak > 1e-9 {
        let inv = 0.95 / peak;
        for x in &mut s {
            *x *= inv;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sources_separates_rows() {
        // 2 sources of length 3, row-major: [s0_0,s0_1,s0_2, s1_0,s1_1,s1_2]
        let flat = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let s = split_sources(&flat, 2, 3);
        assert_eq!(s, vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
    }

    #[test]
    fn split_sources_single() {
        assert_eq!(split_sources(&[7.0, 8.0], 1, 2), vec![vec![7.0, 8.0]]);
    }

    #[test]
    fn peak_normalize_scales_to_unit_headroom() {
        // peak 400000 → scaled so max abs ≈ 0.95
        let out = peak_normalize(vec![400_000.0, -200_000.0, 0.0]);
        let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((peak - 0.95).abs() < 1e-4, "peak was {peak}");
        // ratios preserved
        assert!((out[1] / out[0] + 0.5).abs() < 1e-5);
    }

    #[test]
    fn peak_normalize_passes_silence() {
        assert_eq!(peak_normalize(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }
}
