//! Audio loading. studio extracts a 16 kHz mono WAV before calling /analyze, so
//! we only read + downmix (no resampling); a differing rate is logged, not fixed.

use anyhow::{Context, Result};

pub const SR: u32 = 16_000;

pub fn load_wav_16k_mono(path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("mở wav {path}"))?;
    let spec = reader.spec();
    if spec.sample_rate != SR {
        tracing::warn!("expected {SR} Hz audio, got {} Hz", spec.sample_rate);
    }
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|v| v as f32 / max)
                .collect()
        }
    };
    if channels > 1 {
        Ok(interleaved
            .chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect())
    } else {
        Ok(interleaved)
    }
}
