//! Speaker diarization: cluster per-segment wav2vec2 embeddings into speakers.
//!
//! This reuses the (single, ort-based) audeering wav2vec2 model already loaded
//! for age/gender — its pooled hidden state doubles as a speaker embedding. That
//! keeps everything on one ONNX Runtime (no `sherpa-onnx` runtime conflict) and
//! one binary. It clusters at ASR-segment granularity (no overlap handling),
//! which fits sequential multi-speaker video; a dedicated speaker-embedding
//! model could be swapped in later for tougher cases.

use crate::cluster::cluster;

/// Default cosine-similarity threshold for merging segments into one speaker.
/// Tuned against real clips; override with `MEDIA_AI_DIARIZE_THRESHOLD`.
pub const DEFAULT_THRESHOLD: f32 = 0.85;

pub fn speaker_label(i: usize) -> String {
    format!("SPEAKER_{i:02}")
}

/// Cluster one embedding per segment into `SPEAKER_xx` labels (one per input).
pub fn assign_speakers(
    embeddings: &[Vec<f32>],
    threshold: f32,
    num_speakers: Option<usize>,
    max_speakers: Option<usize>,
) -> Vec<String> {
    cluster(embeddings, threshold, num_speakers, max_speakers)
        .into_iter()
        .map(speaker_label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_two_groups_distinctly() {
        let e = vec![
            vec![1.0, 0.0],
            vec![0.96, 0.04],
            vec![0.0, 1.0],
            vec![0.03, 0.97],
        ];
        let s = assign_speakers(&e, 0.5, None, None);
        assert_eq!(s[0], "SPEAKER_00");
        assert_eq!(s[0], s[1]);
        assert_eq!(s[2], s[3]);
        assert_ne!(s[0], s[2]);
    }

    #[test]
    fn empty_input() {
        assert!(assign_speakers(&[], 0.5, None, None).is_empty());
    }
}
