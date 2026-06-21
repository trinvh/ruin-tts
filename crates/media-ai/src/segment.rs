//! Overlapping-speech detection via the pyannote segmentation-3.0 ONNX
//! (pre-exported by sherpa-onnx — `csukuangfj/sherpa-onnx-pyannote-segmentation-3-0`,
//! NOT gated). The model takes a 10 s waveform window `x` [1,1,160000] and emits
//! per-frame powerset logits `y` [1,589,7] over {∅,a,b,c,ab,ac,bc} (3 speakers,
//! max 2 concurrent). We only need the *count* of active speakers per frame
//! (permutation-invariant), so cross-window speaker identity isn't required.

use crate::onnx::OrtAny;
use anyhow::{Context, Result};
use ndarray::{ArrayD, IxDyn};
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;

/// Powerset class → active speakers (pyannote 3.0: 3 speakers, ≤2 concurrent).
pub const POWERSET: [&[usize]; 7] = [&[], &[0], &[1], &[2], &[0, 1], &[0, 2], &[1, 2]];

pub const WINDOW: usize = 160_000; // 10 s @ 16 kHz
pub const FRAME_SHIFT: usize = 270; // samples per output frame
const MIN_OVERLAP_SECS: f64 = 0.2; // ignore sub-200 ms blips

/// Number of active speakers in the argmax powerset class of one frame's logits.
pub fn active_count(logits: &[f32]) -> usize {
    let best = (0..logits.len().min(POWERSET.len()))
        .max_by(|&a, &b| logits[a].total_cmp(&logits[b]))
        .unwrap_or(0);
    POWERSET[best].len()
}

/// Contiguous runs of frames with `count >= 2` → time intervals (frame `i`
/// starts at `i * shift_s`), dropping runs shorter than `min_dur`.
pub fn overlap_intervals(counts: &[usize], shift_s: f64, min_dur: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut run_start: Option<usize> = None;
    for (i, &c) in counts.iter().enumerate() {
        match (c >= 2, run_start) {
            (true, None) => run_start = Some(i),
            (false, Some(s)) => {
                push_if_long(&mut out, s, i, shift_s, min_dur);
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = run_start {
        push_if_long(&mut out, s, counts.len(), shift_s, min_dur);
    }
    out
}

fn push_if_long(out: &mut Vec<(f64, f64)>, start: usize, end: usize, shift_s: f64, min_dur: f64) {
    let (a, b) = (start as f64 * shift_s, end as f64 * shift_s);
    if b - a >= min_dur {
        out.push((round3(a), round3(b)));
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

pub struct Segmenter {
    session: Option<Mutex<Session>>,
}

impl Segmenter {
    /// `model_path` = the pyannote segmentation ONNX; `None` disables overlap
    /// detection (returns no overlaps).
    pub fn load(model_path: Option<&Path>) -> Result<Self> {
        let session = match model_path {
            Some(p) => {
                let s = Session::builder()
                    .any()?
                    .commit_from_file(p)
                    .any()
                    .with_context(|| format!("nạp model segmentation {}", p.display()))?;
                tracing::info!("segmentation: đã nạp {}", p.display());
                Some(Mutex::new(s))
            }
            None => {
                tracing::warn!(
                    "segmentation: chưa cấu hình model (MEDIA_AI_SEGMENT_REPO/PATH) — bỏ phát hiện chồng tiếng"
                );
                None
            }
        };
        Ok(Self { session })
    }

    pub fn enabled(&self) -> bool {
        self.session.is_some()
    }

    /// Overlap intervals (seconds) over the whole clip. Non-overlapping 10 s
    /// windows; the last is zero-padded and trimmed to the real audio length.
    pub fn overlaps(&self, samples: &[f32]) -> Vec<(f64, f64)> {
        let Some(mtx) = &self.session else {
            return Vec::new();
        };
        match self.frame_counts(mtx, samples) {
            Ok(counts) => {
                overlap_intervals(&counts, FRAME_SHIFT as f64 / 16_000.0, MIN_OVERLAP_SECS)
            }
            Err(e) => {
                tracing::warn!("segmentation lỗi: {e}");
                Vec::new()
            }
        }
    }

    fn frame_counts(&self, mtx: &Mutex<Session>, samples: &[f32]) -> Result<Vec<usize>> {
        let mut counts = Vec::new();
        let mut pos = 0;
        while pos < samples.len() {
            let end = (pos + WINDOW).min(samples.len());
            let real = end - pos;
            let mut chunk = samples[pos..end].to_vec();
            chunk.resize(WINDOW, 0.0);
            let frames = self.run_window(mtx, &chunk)?;
            // keep only frames covering real (non-padded) audio
            let keep = (real / FRAME_SHIFT).min(frames.len());
            counts.extend_from_slice(&frames[..keep]);
            pos += WINDOW;
        }
        Ok(counts)
    }

    /// Run one 10 s window → per-frame active-speaker counts.
    fn run_window(&self, mtx: &Mutex<Session>, chunk: &[f32]) -> Result<Vec<usize>> {
        let arr: ArrayD<f32> = ArrayD::from_shape_vec(IxDyn(&[1, 1, chunk.len()]), chunk.to_vec())
            .context("segmentation input tensor")?;
        let feeds: Vec<(Cow<'static, str>, SessionInputValue<'static>)> = vec![(
            Cow::Borrowed("x"),
            SessionInputValue::from(Tensor::from_array(arr).any()?),
        )];
        let mut session = mtx
            .lock()
            .map_err(|_| anyhow::anyhow!("segmentation lock"))?;
        let out = session.run(feeds).any()?;
        let y = out["y"].try_extract_array::<f32>().any()?;
        let shape = y.shape().to_vec(); // [1, num_frames, 7]
        let (nf, nc) = (shape[1], shape[2]);
        let flat: Vec<f32> = y.iter().copied().collect();
        Ok((0..nf)
            .map(|f| active_count(&flat[f * nc..f * nc + nc]))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_count_maps_powerset() {
        // argmax silence (class 0) → 0 active
        assert_eq!(active_count(&[9.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]), 0);
        // class 2 ({1}) → 1 active
        assert_eq!(active_count(&[0.0, 0.0, 9.0, 0.0, 0.0, 0.0, 0.0]), 1);
        // class 4 ({0,1}) → 2 active (overlap)
        assert_eq!(active_count(&[0.0, 0.0, 0.0, 0.0, 9.0, 0.0, 0.0]), 2);
        // class 6 ({1,2}) → 2 active
        assert_eq!(active_count(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 9.0]), 2);
    }

    #[test]
    fn overlap_intervals_groups_contiguous_runs() {
        // shift 1.0 s/frame for easy arithmetic; counts: [1,2,2,1,2,2,2]
        let counts = vec![1, 2, 2, 1, 2, 2, 2];
        let iv = overlap_intervals(&counts, 1.0, 0.0);
        assert_eq!(iv, vec![(1.0, 3.0), (4.0, 7.0)]);
    }

    #[test]
    fn overlap_intervals_drops_short_runs() {
        // one 1-frame run (0.1 s) below the 0.2 s minimum → dropped
        let counts = vec![1, 2, 1, 2, 2, 2];
        let iv = overlap_intervals(&counts, 0.1, 0.2);
        assert_eq!(iv, vec![(0.3, 0.6)]);
    }

    #[test]
    fn overlap_intervals_empty_when_no_overlap() {
        assert!(overlap_intervals(&[0, 1, 1, 0, 1], 1.0, 0.0).is_empty());
    }
}
