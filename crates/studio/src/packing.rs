//! ChunkByDuration: pack whole chapters into videos under a duration budget.
//! Whole chapters only — never split a chapter. A chapter that alone exceeds
//! the narration budget is flagged for manual handling.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterMeta {
    pub number: u32,
    pub word_count: u32,
    #[serde(default)]
    pub title: Option<String>,
    /// Actual rendered narration length, if known — preferred over the estimate.
    #[serde(default)]
    pub est_seconds: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct PackConfig {
    /// Hard per-video cap in seconds (90 min = 5400).
    pub cap_seconds: f64,
    /// Intro + outro + music tails reserved per video.
    pub overhead_seconds: f64,
    /// Words per minute for Vietnamese narration (≈130–160).
    pub wpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPack {
    pub index: usize,
    pub chapters: Vec<ChapterMeta>,
    pub est_seconds: f64,
    pub first: u32,
    pub last: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackResult {
    pub videos: Vec<VideoPack>,
    pub flagged: Vec<ChapterMeta>,
}

/// Narration seconds implied by a word count at a given speaking rate.
pub fn estimate_seconds(word_count: u32, wpm: f64) -> f64 {
    assert!(wpm > 0.0, "wpm must be positive");
    (word_count as f64 / wpm) * 60.0
}

fn chapter_seconds(c: &ChapterMeta, wpm: f64) -> f64 {
    c.est_seconds
        .unwrap_or_else(|| estimate_seconds(c.word_count, wpm))
}

pub fn pack_chapters(chapters: &[ChapterMeta], config: PackConfig) -> PackResult {
    let budget = config.cap_seconds - config.overhead_seconds;
    assert!(budget > 0.0, "overhead must be smaller than the cap");

    let mut videos: Vec<VideoPack> = Vec::new();
    let mut flagged: Vec<ChapterMeta> = Vec::new();
    let mut current: Vec<ChapterMeta> = Vec::new();
    let mut current_secs = 0.0;

    let flush =
        |videos: &mut Vec<VideoPack>, current: &mut Vec<ChapterMeta>, current_secs: &mut f64| {
            if current.is_empty() {
                return;
            }
            let chapters = std::mem::take(current);
            videos.push(VideoPack {
                index: videos.len() + 1,
                first: chapters.first().unwrap().number,
                last: chapters.last().unwrap().number,
                est_seconds: *current_secs,
                chapters,
            });
            *current_secs = 0.0;
        };

    for c in chapters {
        let secs = chapter_seconds(c, config.wpm);
        if secs > budget {
            flush(&mut videos, &mut current, &mut current_secs);
            flagged.push(c.clone());
            continue;
        }
        if current_secs + secs > budget {
            flush(&mut videos, &mut current, &mut current_secs);
        }
        current.push(c.clone());
        current_secs += secs;
    }
    flush(&mut videos, &mut current, &mut current_secs);

    PackResult { videos, flagged }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(number: u32, word_count: u32) -> ChapterMeta {
        ChapterMeta {
            number,
            word_count,
            title: None,
            est_seconds: None,
        }
    }
    const CFG: PackConfig = PackConfig {
        cap_seconds: 600.0,
        overhead_seconds: 0.0,
        wpm: 150.0,
    };

    #[test]
    fn estimate_from_words() {
        assert!((estimate_seconds(150, 150.0) - 60.0).abs() < 1e-9);
        assert!((estimate_seconds(300, 150.0) - 120.0).abs() < 1e-9);
    }

    #[test]
    fn packs_greedily_to_budget() {
        let r = pack_chapters(&[ch(1, 300), ch(2, 300), ch(3, 300), ch(4, 300)], CFG);
        assert_eq!(r.videos.len(), 1);
        assert_eq!(nums(&r.videos[0]), vec![1, 2, 3, 4]);
        assert!(r.flagged.is_empty());
    }

    #[test]
    fn new_video_when_exceeding() {
        let chs: Vec<_> = (1..=6).map(|n| ch(n, 300)).collect();
        let r = pack_chapters(&chs, CFG);
        assert_eq!(r.videos.len(), 2);
        assert_eq!(nums(&r.videos[0]), vec![1, 2, 3, 4, 5]);
        assert_eq!(nums(&r.videos[1]), vec![6]);
        assert_eq!(r.videos[1].index, 2);
    }

    #[test]
    fn never_splits_a_chapter() {
        let chs: Vec<_> = (1..=7).map(|n| ch(n, 500)).collect();
        let r = pack_chapters(&chs, CFG);
        let mut all: Vec<u32> = r
            .videos
            .iter()
            .flat_map(|v| v.chapters.iter().map(|c| c.number))
            .collect();
        all.sort();
        assert_eq!(all, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn flags_oversize_chapter() {
        let r = pack_chapters(&[ch(1, 300), ch(2, 5000), ch(3, 300)], CFG);
        assert_eq!(
            r.flagged.iter().map(|c| c.number).collect::<Vec<_>>(),
            vec![2]
        );
        let packed: Vec<u32> = r
            .videos
            .iter()
            .flat_map(|v| v.chapters.iter().map(|c| c.number))
            .collect();
        assert_eq!(packed, vec![1, 3]);
    }

    #[test]
    fn overhead_reduces_budget() {
        let cfg = PackConfig {
            cap_seconds: 600.0,
            overhead_seconds: 200.0,
            wpm: 150.0,
        };
        let chs: Vec<_> = (1..=4).map(|n| ch(n, 300)).collect();
        let r = pack_chapters(&chs, cfg);
        assert_eq!(nums(&r.videos[0]), vec![1, 2, 3]);
        assert_eq!(nums(&r.videos[1]), vec![4]);
    }

    #[test]
    fn actual_est_seconds_wins() {
        let a = ChapterMeta {
            number: 1,
            word_count: 100,
            title: None,
            est_seconds: Some(590.0),
        };
        let b = ChapterMeta {
            number: 2,
            word_count: 100,
            title: None,
            est_seconds: Some(590.0),
        };
        let r = pack_chapters(&[a, b], CFG);
        assert_eq!(r.videos.len(), 2);
    }

    fn nums(v: &VideoPack) -> Vec<u32> {
        v.chapters.iter().map(|c| c.number).collect()
    }
}
