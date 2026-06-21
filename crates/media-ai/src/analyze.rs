//! Orchestration: ASR + diarization + age/gender → dubbing-ready segments.
//! A faithful port of the Python `analyze.py`.

use crate::agegender::AgeGenderModel;
use crate::asr::Asr;
use crate::audio::{self, SR};
use crate::diarize::{Diarizer, Turn};
use crate::types::{AnalyzeResponse, Segment, Speaker};
use anyhow::Result;
use std::collections::BTreeSet;

pub struct Analyzer {
    asr: Asr,
    diarizer: Diarizer,
    agegender: AgeGenderModel,
}

impl Analyzer {
    pub fn new(asr: Asr, diarizer: Diarizer, agegender: AgeGenderModel) -> Self {
        Self {
            asr,
            diarizer,
            agegender,
        }
    }

    pub fn analyze(
        &self,
        audio_path: &str,
        hint_lang: Option<&str>,
        num_speakers: Option<u32>,
    ) -> Result<AnalyzeResponse> {
        let samples = audio::load_wav_16k_mono(audio_path)?;
        let duration = samples.len() as f64 / SR as f64;

        let asr = self.asr.transcribe(&samples, hint_lang)?;
        let turns = self.diarizer.diarize(&samples, duration, num_speakers)?;

        let segments: Vec<Segment> = asr
            .segments
            .iter()
            .enumerate()
            .map(|(i, seg)| Segment {
                id: i as i64,
                start: round3(seg.start),
                end: round3(seg.end),
                speaker: speaker_for(seg.start, seg.end, &turns),
                text_src: seg.text.clone(),
                lang: asr.language.clone(),
            })
            .collect();

        // Per-speaker age/gender on a representative concatenation.
        let speaker_ids: BTreeSet<&str> = segments.iter().map(|s| s.speaker.as_str()).collect();
        let mut speakers = Vec::new();
        let mut any_gender = false;
        for spk in speaker_ids {
            let samp = speaker_samples(&samples, &segments, spk, 12.0);
            let ag = self.agegender.predict(&samp, SR);
            any_gender |= ag.gender.is_some();
            speakers.push(Speaker {
                speaker: spk.to_string(),
                gender: ag.gender,
                age: ag.age,
            });
        }

        let gender_note = (!any_gender && !speakers.is_empty())
            .then(|| "age/gender model chưa được cấu hình".to_string());

        Ok(AnalyzeResponse {
            language: asr.language,
            segments,
            speakers,
            gender_note,
        })
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Diarization speaker with the greatest temporal overlap with [start, end].
fn speaker_for(start: f64, end: f64, turns: &[Turn]) -> String {
    let mut best = "SPEAKER_00".to_string();
    let mut best_ov = 0.0;
    for t in turns {
        let ov = (end.min(t.end) - start.max(t.start)).max(0.0);
        if ov > best_ov {
            best_ov = ov;
            best = t.speaker.clone();
        }
    }
    best
}

/// Concatenate up to `max_seconds` of one speaker's longest segments.
fn speaker_samples(audio: &[f32], segs: &[Segment], speaker: &str, max_seconds: f64) -> Vec<f32> {
    let mut own: Vec<&Segment> = segs.iter().filter(|s| s.speaker == speaker).collect();
    own.sort_by(|a, b| (b.end - b.start).total_cmp(&(a.end - a.start)));
    let mut out = Vec::new();
    let mut total = 0.0;
    for s in own {
        let a = (s.start * SR as f64) as usize;
        let b = ((s.end * SR as f64) as usize).min(audio.len());
        if a < b {
            out.extend_from_slice(&audio[a..b]);
        }
        total += s.end - s.start;
        if total >= max_seconds {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: i64, start: f64, end: f64, spk: &str) -> Segment {
        Segment {
            id,
            start,
            end,
            speaker: spk.into(),
            text_src: String::new(),
            lang: "vi".into(),
        }
    }
    fn turn(start: f64, end: f64, spk: &str) -> Turn {
        Turn {
            start,
            end,
            speaker: spk.into(),
        }
    }

    #[test]
    fn speaker_for_picks_greatest_overlap() {
        let turns = vec![turn(0.0, 5.0, "A"), turn(5.0, 10.0, "B")];
        assert_eq!(speaker_for(1.0, 2.0, &turns), "A");
        assert_eq!(speaker_for(6.0, 9.0, &turns), "B");
        // straddles the boundary but leans into B
        assert_eq!(speaker_for(4.5, 8.0, &turns), "B");
    }

    #[test]
    fn speaker_for_defaults_when_no_overlap() {
        assert_eq!(speaker_for(1.0, 2.0, &[]), "SPEAKER_00");
        assert_eq!(
            speaker_for(20.0, 21.0, &[turn(0.0, 5.0, "A")]),
            "SPEAKER_00"
        );
    }

    #[test]
    fn speaker_samples_takes_longest_segments_of_that_speaker() {
        let audio = vec![0.1f32; 10 * SR as usize]; // 10 s
        let segs = vec![
            seg(0, 0.0, 1.0, "A"),
            seg(1, 2.0, 5.0, "A"),
            seg(2, 6.0, 7.0, "B"),
        ];
        // speaker A owns 1 s + 3 s = 4 s
        let out = speaker_samples(&audio, &segs, "A", 12.0);
        assert_eq!(out.len(), 4 * SR as usize);
    }

    #[test]
    fn speaker_samples_stops_at_the_cap() {
        let audio = vec![0.1f32; 20 * SR as usize];
        let segs = vec![seg(0, 0.0, 8.0, "A"), seg(1, 9.0, 17.0, "A")];
        // longest-first: 8 s (total 8 < 12), +8 s (total 16 ≥ 12, stop) → 16 s
        let out = speaker_samples(&audio, &segs, "A", 12.0);
        assert_eq!(out.len(), 16 * SR as usize);
    }

    #[test]
    fn round3_rounds_to_milliseconds() {
        assert_eq!(round3(1.23456), 1.235);
        assert_eq!(round3(0.0), 0.0);
    }
}
