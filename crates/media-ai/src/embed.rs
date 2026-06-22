//! Speaker embedding via the WavLM-base-plus-sv ONNX (raw 16 kHz waveform →
//! L2-normalized 512-d x-vector). Far more discriminative for diarization than
//! the age/gender hidden state — separates same-gender / many speakers. Export
//! with tools/export-speaker-embedding-onnx.py; point at it via MEDIA_AI_EMBED_*.

use crate::onnx::{output_name_excluding, run_waveform, OrtAny};
use anyhow::{Context, Result};
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;

pub struct Embedder {
    // ort's Session::run takes &mut self → Mutex for interior mutability.
    session: Option<Mutex<Session>>,
    output: String,
}

impl Embedder {
    /// `model_path` = the exported WavLM ONNX; `None` disables embeddings, so
    /// diarization falls back to a single speaker.
    pub fn load(model_path: Option<&Path>) -> Result<Self> {
        match model_path {
            Some(p) => {
                let s = Session::builder()
                    .any()?
                    .commit_from_file(p)
                    .any()
                    .with_context(|| format!("nạp model speaker-embedding {}", p.display()))?;
                let output = output_name_excluding(&s, &[]);
                tracing::info!(
                    "speaker-embedding: đã nạp {} (output={output})",
                    p.display()
                );
                Ok(Self {
                    session: Some(Mutex::new(s)),
                    output,
                })
            }
            None => {
                tracing::warn!(
                    "speaker-embedding: chưa cấu hình model (MEDIA_AI_EMBED_REPO/PATH) — diarization dùng 1 người"
                );
                Ok(Self {
                    session: None,
                    output: String::new(),
                })
            }
        }
    }

    /// Embed one segment's audio → a 512-d unit vector, or `None` (no model /
    /// empty / inference error — caller degrades to a single speaker).
    pub fn infer(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let mtx = self.session.as_ref()?;
        if samples.is_empty() {
            return None;
        }
        // The TDNN/conv stack has a receptive field of a few hundred ms; a tiny
        // segment (e.g. a 5-sample diarization sliver) makes the conv fail with
        // "Invalid input shape". Zero-pad up to a safe minimum (~1s @ 16 kHz) so
        // short-but-real segments still get an embedding instead of being dropped.
        const MIN_SAMPLES: usize = 16_000;
        let padded: Vec<f32>;
        let samples: &[f32] = if samples.len() < MIN_SAMPLES {
            padded = samples
                .iter()
                .copied()
                .chain(std::iter::repeat(0.0))
                .take(MIN_SAMPLES)
                .collect();
            &padded
        } else {
            samples
        };
        let res = run_waveform(mtx, samples, |out| {
            let v = out
                .get(self.output.as_str())
                .ok_or_else(|| anyhow::anyhow!("thiếu output embedding '{}'", self.output))?;
            Ok(v.try_extract_array::<f32>()
                .any()?
                .iter()
                .copied()
                .collect::<Vec<f32>>())
        });
        match res {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::warn!("embedding inference lỗi: {e}");
                None
            }
        }
    }
}
