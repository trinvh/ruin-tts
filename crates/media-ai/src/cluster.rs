//! Agglomerative (average-linkage, cosine) clustering of speaker embeddings —
//! the pure core of diarization. Given one embedding per ASR segment, group
//! them into speakers either by a similarity threshold or a fixed count.

/// Cosine similarity in [-1, 1]; 0 if either vector has zero norm.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na < 1e-9 || nb < 1e-9 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Cluster `embeddings` into speaker ids (0-based, contiguous). Merge the two
/// most-similar clusters (average linkage) while the best similarity ≥
/// `threshold`; if `num_speakers` is given, merge until exactly that many
/// remain (ignoring the threshold).
pub fn cluster(embeddings: &[Vec<f32>], threshold: f32, num_speakers: Option<usize>) -> Vec<usize> {
    unimplemented!("RED")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distinct(labels: &[usize]) -> usize {
        let mut v = labels.to_vec();
        v.sort_unstable();
        v.dedup();
        v.len()
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn two_separated_groups() {
        let e = vec![
            vec![1.0, 0.0],
            vec![0.95, 0.05],
            vec![0.0, 1.0],
            vec![0.05, 0.95],
        ];
        let labels = cluster(&e, 0.5, None);
        assert_eq!(labels[0], labels[1]);
        assert_eq!(labels[2], labels[3]);
        assert_ne!(labels[0], labels[2]);
        assert_eq!(distinct(&labels), 2);
    }

    #[test]
    fn all_similar_one_cluster() {
        let e = vec![vec![1.0, 0.0], vec![0.99, 0.01], vec![0.98, 0.0]];
        assert_eq!(distinct(&cluster(&e, 0.5, None)), 1);
    }

    #[test]
    fn num_speakers_forces_k() {
        // pairwise sims all < 0.95 → threshold alone gives 3 clusters; Some(2) forces 2.
        let e = vec![vec![1.0, 0.0], vec![0.7, 0.7], vec![0.0, 1.0]];
        assert_eq!(distinct(&cluster(&e, 0.95, None)), 3);
        assert_eq!(distinct(&cluster(&e, 0.95, Some(2))), 2);
    }

    #[test]
    fn empty_and_single() {
        assert!(cluster(&[], 0.5, None).is_empty());
        assert_eq!(cluster(&[vec![1.0, 0.0]], 0.5, None), vec![0]);
    }
}
