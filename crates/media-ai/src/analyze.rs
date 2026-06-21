//! Orchestration: ASR → per-segment wav2vec2 features → cluster into speakers →
//! per-speaker age/gender. Replaces the Python `analyze.py` flow (which used a
//! separate pyannote pass) with embedding clustering over the ASR segments.

use crate::agegender::{SegmentFeatures, Wav2Vec2};
use crate::asr::Asr;
use crate::audio::{self, SR};
use crate::diarize::assign_speakers;
use crate::types::{AnalyzeResponse, Segment, Speaker};
use anyhow::Result;
use std::collections::BTreeMap;

pub struct Analyzer {
    asr: Asr,
    model: Wav2Vec2,
    threshold: f32,
}

impl Analyzer {
    pub fn new(asr: Asr, model: Wav2Vec2, threshold: f32) -> Self {
        Self {
            asr,
            model,
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

        // Per-segment wav2vec2 features (embedding + age/gender), when the model
        // is loaded — `None` per segment otherwise.
        let feats: Vec<Option<SegmentFeatures>> = asr
            .segments
            .iter()
            .map(|s| {
                let a = (s.start * SR as f64) as usize;
                let b = ((s.end * SR as f64) as usize).min(samples.len());
                if a < b {
                    self.model.infer(&samples[a..b])
                } else {
                    None
                }
            })
            .collect();

        // Diarization: cluster the segments that produced an embedding.
        let with_emb: Vec<usize> = feats
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|_| i))
            .collect();
        let mut speaker_of = vec!["SPEAKER_00".to_string(); asr.segments.len()];
        if !with_emb.is_empty() {
            let embs: Vec<Vec<f32>> = with_emb
                .iter()
                .map(|&i| feats[i].as_ref().unwrap().embedding.clone())
                .collect();
            let labels = assign_speakers(&embs, self.threshold, num_speakers.map(|n| n as usize));
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

        // Per-speaker age/gender: duration-weighted aggregate of segment features.
        let mut by_speaker: BTreeMap<String, Vec<(f64, Option<f64>, Option<String>)>> =
            BTreeMap::new();
        for s in &segments {
            by_speaker.entry(s.speaker.clone()).or_default();
        }
        for (i, seg) in asr.segments.iter().enumerate() {
            let dur = (seg.end - seg.start).max(0.01);
            let (age, gender) = match &feats[i] {
                Some(f) => (f.age, f.gender.clone()),
                None => (None, None),
            };
            by_speaker
                .get_mut(&speaker_of[i])
                .unwrap()
                .push((dur, age, gender));
        }
        let speakers: Vec<Speaker> = by_speaker
            .into_iter()
            .map(|(speaker, items)| {
                let (age, gender) = aggregate(&items);
                Speaker {
                    speaker,
                    gender,
                    age,
                }
            })
            .collect();

        let any_gender = speakers.iter().any(|s| s.gender.is_some());
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

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Duration-weighted aggregate: mean age over segments that have one, and the
/// gender with the most total speech duration.
fn aggregate(items: &[(f64, Option<f64>, Option<String>)]) -> (Option<f64>, Option<String>) {
    let mut age_sum = 0.0;
    let mut age_w = 0.0;
    let mut votes: BTreeMap<String, f64> = BTreeMap::new();
    for (dur, age, gender) in items {
        if let Some(a) = age {
            age_sum += a * dur;
            age_w += dur;
        }
        if let Some(g) = gender {
            *votes.entry(g.clone()).or_insert(0.0) += dur;
        }
    }
    let age = (age_w > 0.0).then(|| round1(age_sum / age_w));
    let gender = votes
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(g, _)| g);
    (age, gender)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round3_rounds_to_milliseconds() {
        assert_eq!(round3(1.23456), 1.235);
        assert_eq!(round3(0.0), 0.0);
    }

    #[test]
    fn aggregate_weights_age_by_duration() {
        let items = vec![
            (2.0, Some(30.0), Some("male".to_string())),
            (1.0, Some(60.0), Some("male".to_string())),
        ];
        let (age, gender) = aggregate(&items);
        assert_eq!(age, Some(40.0)); // (30*2 + 60*1) / 3
        assert_eq!(gender.as_deref(), Some("male"));
    }

    #[test]
    fn aggregate_gender_is_duration_majority() {
        let items = vec![
            (3.0, None, Some("female".to_string())),
            (1.0, None, Some("male".to_string())),
        ];
        let (age, gender) = aggregate(&items);
        assert_eq!(age, None);
        assert_eq!(gender.as_deref(), Some("female"));
    }

    #[test]
    fn aggregate_empty_is_none() {
        assert_eq!(aggregate(&[]), (None, None));
        assert_eq!(aggregate(&[(1.0, None, None)]), (None, None));
    }
}
