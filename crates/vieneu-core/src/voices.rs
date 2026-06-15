//! Built-in preset voices for the v3-Turbo emotion checkpoint.
//!
//! Each preset carries a speaker reserved token (`reserved_id`, ids 13..42)
//! plus fixed MOSS reference codes `(T, n_vq)`. The asset ships embedded.

use anyhow::{anyhow, Result};
use ndarray::Array2;
use serde::Deserialize;
use std::collections::BTreeMap;

static VOICES_JSON: &str = include_str!("../assets/voices_v3_turbo.json");

#[derive(Debug, Deserialize)]
struct VoicesFile {
    default_voice: Option<String>,
    presets: BTreeMap<String, PresetRaw>,
}

#[derive(Debug, Deserialize)]
struct PresetRaw {
    reserved_id: Option<i64>,
    #[serde(default)]
    description: String,
    codes: Vec<Vec<i64>>,
}

/// A resolved preset voice ready for prompt building.
#[derive(Debug, Clone)]
pub struct PresetVoice {
    pub name: String,
    pub description: String,
    /// Speaker reserved token; `None` falls back to the emotion-tag path.
    pub reserved_id: Option<i64>,
    /// Reference codes `(T, n_vq)`.
    pub codes: Array2<i64>,
}

/// The set of built-in voices plus the default selection.
#[derive(Debug, Clone, Default)]
pub struct VoiceBook {
    voices: BTreeMap<String, PresetVoice>,
    default_voice: Option<String>,
}

impl VoiceBook {
    /// Load the embedded preset voices.
    pub fn load_embedded() -> Result<Self> {
        Self::from_json(VOICES_JSON)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let parsed: VoicesFile = serde_json::from_str(json)?;
        let mut voices = BTreeMap::new();
        for (name, raw) in parsed.presets {
            let t = raw.codes.len();
            let n_vq = raw.codes.first().map(|r| r.len()).unwrap_or(0);
            let flat: Vec<i64> = raw.codes.into_iter().flatten().collect();
            let codes = Array2::from_shape_vec((t, n_vq), flat)
                .map_err(|e| anyhow!("voice {name} codes reshape: {e}"))?;
            voices.insert(
                name.clone(),
                PresetVoice {
                    name: name.clone(),
                    description: raw.description,
                    reserved_id: raw.reserved_id,
                    codes,
                },
            );
        }
        Ok(Self {
            voices,
            default_voice: parsed.default_voice,
        })
    }

    pub fn default_name(&self) -> Option<&str> {
        self.default_voice.as_deref()
    }

    pub fn get(&self, name: &str) -> Option<&PresetVoice> {
        self.voices.get(name)
    }

    pub fn default_voice(&self) -> Option<&PresetVoice> {
        self.default_voice.as_ref().and_then(|n| self.voices.get(n))
    }

    /// `[(label, voice_id), ...]` — label is `"name — description"`.
    pub fn list(&self) -> Vec<(String, String)> {
        self.voices
            .values()
            .map(|v| {
                let label = if v.description.is_empty() {
                    v.name.clone()
                } else {
                    format!("{} — {}", v.name, v.description)
                };
                (label, v.name.clone())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_voices() {
        let book = VoiceBook::load_embedded().unwrap();
        assert!(book.list().len() >= 5);
        assert_eq!(book.default_name(), Some("Ngọc Linh"));
        let v = book.default_voice().unwrap();
        assert_eq!(v.codes.ncols(), 16);
        assert!(v.reserved_id.is_some());
    }
}
