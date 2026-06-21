//! Orchestration: ASR → per-segment WavLM speaker embedding → cluster into
//! speakers → per-speaker age/gender. Replaces the Python `analyze.py` flow
//! (which used a separate pyannote pass).

use crate::agegender::AgeGenderModel;
use crate::asr::Asr;
use crate::audio::{self, SR};
use crate::diarize::assign_speakers;
use crate::embed::Embedder;
use crate::segment::Segmenter;
use crate::types::{AnalyzeResponse, OverlapSpan, Segment, Speaker};
use anyhow::Result;
use std::collections::BTreeSet;

/// Cap on the audio fed to the age/gender model per speaker (seconds).
const AGEGENDER_MAX_SECS: f64 = 12.0;

pub struct Analyzer {
    asr: Asr,
    embedder: Embedder,
    agegender: AgeGenderModel,
    segmenter: Segmenter,
    threshold: f32,
}

impl Analyzer {
    pub fn new(
        asr: Asr,
        embedder: Embedder,
        agegender: AgeGenderModel,
        segmenter: Segmenter,
        threshold: f32,
    ) -> Self {
        Self {
            asr,
            embedder,
            agegender,
            segmenter,
            threshold,
        }
    }

    pub fn analyze(
        &self,
        audio_path: &str,
        hint_lang: Option<&str>,
        num_speakers: Option<u32>,
    ) -> Result<AnalyzeResponse> {
        let samples = audio::load_wav_16k_mono(audio_path)?;
        let asr = self.asr.transcribe(&samples, hint_lang)?;

        // Per-segment speaker embedding (WavLM), when the model is loaded.
        let embs: Vec<Option<Vec<f32>>> = asr
            .segments
            .iter()
            .map(|s| {
                let slice = slice_for(&samples, s.start, s.end);
                if slice.is_empty() {
                    None
                } else {
                    self.embedder.infer(slice)
                }
            })
            .collect();

        // Diarization: cluster the segments that produced an embedding.
        let with_emb: Vec<usize> = embs
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.as_ref().map(|_| i))
            .collect();
        let mut speaker_of = vec!["SPEAKER_00".to_string(); asr.segments.len()];
        if !with_emb.is_empty() {
            let vecs: Vec<Vec<f32>> = with_emb.iter().map(|&i| embs[i].clone().unwrap()).collect();
            let labels = assign_speakers(&vecs, self.threshold, num_speakers.map(|n| n as usize));
            for (k, &i) in with_emb.iter().enumerate() {
                speaker_of[i] = labels[k].clone();
            }
        }

        let segments: Vec<Segment> = asr
            .segments
            .iter()
            .enumerate()
            .map(|(i, seg)| Segment {
                id: i as i64,
                start: round3(seg.start),
                end: round3(seg.end),
                speaker: speaker_of[i].clone(),
                text_src: seg.text.clone(),
                lang: asr.language.clone(),
            })
            .collect();

        // Per-speaker age/gender on that speaker's concatenated audio.
        let speaker_ids: BTreeSet<&str> = segments.iter().map(|s| s.speaker.as_str()).collect();
        let speakers: Vec<Speaker> = speaker_ids
            .into_iter()
            .map(|spk| {
                let clip = speaker_samples(&samples, &segments, spk, AGEGENDER_MAX_SECS);
                let ag = self.agegender.predict(&clip);
                Speaker {
                    speaker: spk.to_string(),
                    gender: ag.gender,
                    age: ag.age,
                }
            })
            .collect();

        let any_gender = speakers.iter().any(|s| s.gender.is_some());
        let gender_note = (!any_gender && !speakers.is_empty())
            .then(|| "age/gender model chưa được cấu hình".to_string());

        // Overlapping-speech spans (pyannote segmentation), for the dub export.
        let overlaps = self
            .segmenter
            .overlaps(&samples)
            .into_iter()
            .map(|(start, end)| OverlapSpan { start, end })
            .collect();

        Ok(AnalyzeResponse {
            language: asr.language,
            segments,
            speakers,
            gender_note,
            overlaps,
        })
    }
}

fn slice_for(samples: &[f32], start: f64, end: f64) -> &[f32] {
    let a = (start * SR as f64).max(0.0) as usize;
    let b = ((end * SR as f64) as usize).min(samples.len());
    if a < b {
        &samples[a..b]
    } else {
        &[]
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Concatenate a speaker's segment audio (in order), up to `max_secs`, for the
/// age/gender pass. Returns an empty vec if the speaker has no usable audio.
fn speaker_samples(
    samples: &[f32],
    segments: &[Segment],
    speaker: &str,
    max_secs: f64,
) -> Vec<f32> {
    let budget = (max_secs * SR as f64) as usize;
    let mut out: Vec<f32> = Vec::new();
    for seg in segments.iter().filter(|s| s.speaker == speaker) {
        if out.len() >= budget {
            break;
        }
        let slice = slice_for(samples, seg.start, seg.end);
        let take = slice.len().min(budget - out.len());
        out.extend_from_slice(&slice[..take]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: i64, start: f64, end: f64, speaker: &str) -> Segment {
        Segment {
            id,
            start,
            end,
            speaker: speaker.into(),
            text_src: String::new(),
            lang: String::new(),
        }
    }

    #[test]
    fn round3_rounds_to_milliseconds() {
        assert_eq!(round3(1.23456), 1.235);
        assert_eq!(round3(0.0), 0.0);
    }

    #[test]
    fn speaker_samples_concatenates_only_that_speaker() {
        // 3 s of audio at 16 kHz; ramp so we can tell regions apart.
        let samples: Vec<f32> = (0..SR * 3).map(|i| i as f32).collect();
        let segs = vec![
            seg(0, 0.0, 1.0, "SPEAKER_00"),
            seg(1, 1.0, 2.0, "SPEAKER_01"),
            seg(2, 2.0, 3.0, "SPEAKER_00"),
        ];
        let s0 = speaker_samples(&samples, &segs, "SPEAKER_00", 10.0);
        // Two 1 s segments for SPEAKER_00.
        assert_eq!(s0.len(), SR as usize * 2);
        // First sample is from t=0, and the join is at the start of t=2 s.
        assert_eq!(s0[0], 0.0);
        assert_eq!(s0[SR as usize], (SR * 2) as f32);
    }

    #[test]
    fn speaker_samples_respects_the_cap() {
        let samples: Vec<f32> = vec![1.0; SR as usize * 5];
        let segs = vec![seg(0, 0.0, 5.0, "SPEAKER_00")];
        let s = speaker_samples(&samples, &segs, "SPEAKER_00", 2.0);
        assert_eq!(s.len(), SR as usize * 2); // capped at 2 s
    }

    #[test]
    fn speaker_samples_empty_when_no_match() {
        let samples = vec![1.0; SR as usize];
        let segs = vec![seg(0, 0.0, 1.0, "SPEAKER_00")];
        assert!(speaker_samples(&samples, &segs, "SPEAKER_09", 10.0).is_empty());
    }
}
