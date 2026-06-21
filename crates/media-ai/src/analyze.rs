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
            .then(|| "age/gender chưa được port sang Rust".to_string());

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
