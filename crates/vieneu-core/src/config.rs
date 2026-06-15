//! Model configuration parsed from the v3-Turbo `config.json`.

use serde::Deserialize;

/// Token ids and shapes the engine needs. Field names mirror `config.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub n_vq: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub audio_pad_token_id: i64,
    pub text_prompt_start_token_id: i64,
    pub text_prompt_end_token_id: i64,
    pub speech_generation_start_token_id: i64,
    pub speech_generation_end_token_id: i64,
    pub audio_ref_slot_token_id: i64,
    pub emotion_0_token_id: i64,
    pub emotion_4_token_id: i64,
    pub text_vocab_size: usize,
    #[serde(default = "default_local_heads")]
    pub local_num_attention_heads: usize,
    #[serde(default = "default_sample_rate")]
    pub audio_sample_rate: u32,
}

fn default_local_heads() -> usize {
    8
}
fn default_sample_rate() -> u32 {
    48_000
}

impl ModelConfig {
    /// Hidden dim per local (acoustic) attention head — mirrors the Python
    /// engine's `hd_loc = hidden // local_num_attention_heads` (NOT `head_dim`).
    pub fn local_head_dim(&self) -> usize {
        self.hidden_size / self.local_num_attention_heads
    }

    pub fn from_json_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }
}
