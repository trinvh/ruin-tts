//! Merge media-ai's per-speaker overlap transcripts into the dub segment list.
//! Inside an overlap region the plain ASR produced one garbled (mixed) segment;
//! source separation recovered a clean transcript per speaker. We drop the
//! garbled segment(s) and insert one clean segment per separated stream so each
//! simultaneous speaker is translated, voiced and subtitled independently.

use super::clients::{AnalyzedOverlap, AnalyzedSegment};

/// A flat segment (time + speaker + source text) before it becomes a DubSegment.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatSeg {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text_src: String,
}

fn intersection(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Replace base segments that are mostly (>50%) inside an overlap-with-texts with
/// one clean segment per separated stream (stream `i` → `SPEAKER_{i:02}`).
/// Time-ordered. Overlaps without texts (no separation model) are ignored, so
/// the result is just the base segments unchanged.
pub fn merge_overlap_segments(
    base: &[AnalyzedSegment],
    overlaps: &[AnalyzedOverlap],
) -> Vec<FlatSeg> {
    let active: Vec<&AnalyzedOverlap> = overlaps.iter().filter(|o| !o.texts.is_empty()).collect();

    let mut out: Vec<FlatSeg> = Vec::new();
    for s in base {
        let dur = (s.end - s.start).max(1e-6);
        let covered: f64 = active
            .iter()
            .map(|o| intersection(s.start, s.end, o.start, o.end))
            .sum();
        if covered / dur <= 0.5 {
            out.push(FlatSeg {
                start: s.start,
                end: s.end,
                speaker: s.speaker.clone(),
                text_src: s.text_src.clone(),
            });
        }
    }
    for o in &active {
        for (i, t) in o.texts.iter().enumerate() {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            out.push(FlatSeg {
                start: o.start,
                end: o.end,
                speaker: format!("SPEAKER_{i:02}"),
                text_src: t.to_string(),
            });
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.speaker.cmp(&b.speaker)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: i64, start: f64, end: f64, spk: &str, text: &str) -> AnalyzedSegment {
        AnalyzedSegment {
            id,
            start,
            end,
            speaker: spk.into(),
            text_src: text.into(),
        }
    }
    fn ov(start: f64, end: f64, texts: &[&str]) -> AnalyzedOverlap {
        AnalyzedOverlap {
            start,
            end,
            texts: texts.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn replaces_garbled_overlap_with_per_speaker_segments() {
        let base = vec![
            seg(0, 0.0, 2.5, "SPEAKER_00", "intro by A"),
            seg(1, 2.5, 6.5, "SPEAKER_00", "garbled both at once"), // mostly inside overlap
            seg(2, 6.5, 9.0, "SPEAKER_01", "outro by B"),
        ];
        let overlaps = vec![ov(2.9, 6.3, &["clean A line", "clean B line"])];
        let out = merge_overlap_segments(&base, &overlaps);

        // garbled seg dropped; A-intro + 2 overlap streams + B-outro = 4
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].text_src, "intro by A");
        // the two overlap streams, attributed to distinct speakers, same time span
        assert_eq!(out[1].speaker, "SPEAKER_00");
        assert_eq!(out[1].text_src, "clean A line");
        assert_eq!(out[2].speaker, "SPEAKER_01");
        assert_eq!(out[2].text_src, "clean B line");
        assert_eq!((out[1].start, out[1].end), (2.9, 6.3));
        assert_eq!(out[3].text_src, "outro by B");
    }

    #[test]
    fn keeps_segments_only_partially_in_overlap() {
        // base seg [0,10], overlap [9,9.5] covers 0.5/10 = 5% → kept
        let base = vec![seg(0, 0.0, 10.0, "SPEAKER_00", "long")];
        let out = merge_overlap_segments(&base, &[ov(9.0, 9.5, &["x", "y"])]);
        assert!(out.iter().any(|f| f.text_src == "long"));
    }

    #[test]
    fn no_texts_means_unchanged() {
        let base = vec![
            seg(0, 0.0, 2.0, "SPEAKER_00", "a"),
            seg(1, 2.0, 4.0, "SPEAKER_01", "b"),
        ];
        let out = merge_overlap_segments(&base, &[ov(0.5, 3.5, &[])]); // detected but no separation
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text_src, "a");
        assert_eq!(out[1].text_src, "b");
    }
}
