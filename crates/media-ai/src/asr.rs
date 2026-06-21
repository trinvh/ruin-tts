//! Speech recognition with segment timestamps + language detection, via
//! whisper.cpp (whisper-rs). Metal on macOS, CPU elsewhere.

use anyhow::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct AsrSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub struct AsrResult {
    pub language: String,
    pub segments: Vec<AsrSegment>,
}

pub struct Asr {
    ctx: WhisperContext,
}

impl Asr {
    pub fn load(model_path: &str) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .context("nạp model whisper")?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio: &[f32], hint_lang: Option<&str>) -> Result<AsrResult> {
        let mut state = self.ctx.create_state().context("tạo whisper state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(hint_lang); // None → auto-detect
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);

        state.full(params, audio).context("whisper inference")?;

        let mut segments = Vec::new();
        for i in 0..state.full_n_segments() {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };
            let text = seg
                .to_str()
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            // timestamps are in centiseconds (×10 ms).
            segments.push(AsrSegment {
                start: seg.start_timestamp() as f64 / 100.0,
                end: seg.end_timestamp() as f64 / 100.0,
                text,
            });
        }

        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .map(str::to_string)
            .or_else(|| hint_lang.map(str::to_string))
            .unwrap_or_else(|| "auto".into());

        Ok(AsrResult { language, segments })
    }
}
