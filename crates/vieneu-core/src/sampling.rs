//! Token sampling: temperature / top-k / top-p / repetition penalty.
//!
//! Direct port of `OnnxV3LiteEngine._sample` (numpy) so output matches the
//! reference engine. Greedy (`temperature <= 0`) is fully deterministic.

use rand::distributions::{Distribution, WeightedIndex};
use rand::Rng;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    pub repetition_penalty: f32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            top_k: 25,
            top_p: 0.95,
            repetition_penalty: 1.2,
        }
    }
}

fn softmax_inplace(x: &mut [f32]) {
    let max = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for v in x.iter_mut() {
        *v = (*v - max).exp();
        sum += *v;
    }
    if sum > 0.0 {
        for v in x.iter_mut() {
            *v /= sum;
        }
    }
}

fn argmax(x: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in x.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best
}

/// Sample one index from `logits`. `prev` holds indices already chosen for this
/// channel (used by the repetition penalty). Returns the chosen index.
pub fn sample<R: Rng + ?Sized>(
    logits: &[f32],
    params: &SamplingParams,
    prev: Option<&HashSet<usize>>,
    rng: &mut R,
) -> usize {
    let mut l: Vec<f32> = logits.to_vec();

    // Repetition penalty
    if (params.repetition_penalty - 1.0).abs() > f32::EPSILON {
        if let Some(prev) = prev {
            let rp = params.repetition_penalty;
            for &idx in prev {
                if idx < l.len() {
                    let s = l[idx];
                    l[idx] = if s < 0.0 { s * rp } else { s / rp };
                }
            }
        }
    }

    // Greedy
    if !(params.temperature > 0.0) {
        return argmax(&l);
    }

    // Temperature
    let t = params.temperature;
    for v in l.iter_mut() {
        *v /= t;
    }

    // top-k: keep the k largest, mask the rest to -inf
    if params.top_k > 0 && params.top_k < l.len() {
        let k = params.top_k;
        let mut sorted = l.clone();
        sorted.sort_unstable_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let kth = sorted[k - 1];
        for v in l.iter_mut() {
            if *v < kth {
                *v = f32::NEG_INFINITY;
            }
        }
    }

    // top-p (nucleus): sort desc, keep the smallest prefix whose cumulative
    // prob exceeds top_p (matches numpy `(cumsum(p)-p) > top_p`).
    if params.top_p < 1.0 {
        let mut order: Vec<usize> = (0..l.len()).collect();
        order.sort_unstable_by(|&a, &b| {
            l[b].partial_cmp(&l[a]).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut probs: Vec<f32> = order.iter().map(|&i| l[i]).collect();
        softmax_inplace(&mut probs);
        let mut cum = 0.0f32;
        let mut out = vec![f32::NEG_INFINITY; l.len()];
        for (rank, &idx) in order.iter().enumerate() {
            let before = cum; // cumsum - p
            cum += probs[rank];
            if before > params.top_p {
                // removed
            } else {
                out[idx] = l[idx];
            }
        }
        l = out;
    }

    // softmax then categorical sample
    let mut probs = l;
    softmax_inplace(&mut probs);
    match WeightedIndex::new(&probs) {
        Ok(dist) => dist.sample(rng),
        Err(_) => argmax(&probs),
    }
}
