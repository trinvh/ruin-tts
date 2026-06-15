//! Output encoding: WAV (PCM) and MP3 (LAME). Lets the API/CLI emit
//! YouTube-ready compressed audio without an external ffmpeg dependency.

use anyhow::{anyhow, Result};
use mp3lame_encoder::{Builder, FlushNoGap, MonoPcm};

/// Supported output container/codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Wav,
    Mp3,
}

impl OutputFormat {
    /// MIME type for HTTP responses.
    pub fn content_type(self) -> &'static str {
        match self {
            OutputFormat::Wav => "audio/wav",
            OutputFormat::Mp3 => "audio/mpeg",
        }
    }

    /// File extension (no dot).
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Wav => "wav",
            OutputFormat::Mp3 => "mp3",
        }
    }

    /// Parse a case-insensitive format name; `None` if unsupported.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wav" => Some(OutputFormat::Wav),
            "mp3" => Some(OutputFormat::Mp3),
            _ => None,
        }
    }
}

/// Default MP3 bitrate (kbps) — a good quality/size balance for narration.
pub const DEFAULT_MP3_KBPS: u16 = 192;

/// Encode a mono f32 waveform (range roughly [-1, 1]) to an MP3 byte buffer.
pub fn encode_mp3(samples: &[f32], sample_rate: u32, bitrate_kbps: u16) -> Result<Vec<u8>> {
    let pcm: Vec<i16> = samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();

    let mut encoder = Builder::new()
        .ok_or_else(|| anyhow!("failed to create LAME builder"))?
        .with_num_channels(1)
        .map_err(|e| anyhow!("set channels: {e:?}"))?
        .with_sample_rate(sample_rate)
        .map_err(|e| anyhow!("set sample rate: {e:?}"))?
        .with_brate(bitrate_for(bitrate_kbps))
        .map_err(|e| anyhow!("set bitrate: {e:?}"))?
        .with_quality(mp3lame_encoder::Quality::Best)
        .map_err(|e| anyhow!("set quality: {e:?}"))?
        .build()
        .map_err(|e| anyhow!("build LAME encoder: {e:?}"))?;

    let mut out: Vec<u8> = Vec::new();
    out.reserve(mp3lame_encoder::max_required_buffer_size(pcm.len()));

    let n = encoder
        .encode(MonoPcm(&pcm), out.spare_capacity_mut())
        .map_err(|e| anyhow!("mp3 encode: {e:?}"))?;
    // SAFETY: the encoder wrote exactly `n` initialized bytes into the spare capacity.
    unsafe { out.set_len(out.len() + n) };

    let n2 = encoder
        .flush::<FlushNoGap>(out.spare_capacity_mut())
        .map_err(|e| anyhow!("mp3 flush: {e:?}"))?;
    // SAFETY: the flush wrote exactly `n2` initialized bytes into the spare capacity.
    unsafe { out.set_len(out.len() + n2) };

    Ok(out)
}

/// Map a kbps value to the nearest supported LAME constant bitrate.
fn bitrate_for(kbps: u16) -> mp3lame_encoder::Bitrate {
    use mp3lame_encoder::Bitrate::*;
    match kbps {
        0..=104 => Kbps96,
        105..=120 => Kbps112,
        121..=140 => Kbps128,
        141..=176 => Kbps160,
        177..=224 => Kbps192,
        225..=288 => Kbps256,
        _ => Kbps320,
    }
}

/// Encode a mono f32 waveform in the requested format. Returns the bytes and the
/// matching HTTP content-type.
pub fn encode(
    samples: &[f32],
    sample_rate: u32,
    format: OutputFormat,
) -> Result<(Vec<u8>, &'static str)> {
    match format {
        OutputFormat::Wav => Ok((
            crate::audio::wav_bytes_i16(samples, sample_rate)?,
            OutputFormat::Wav.content_type(),
        )),
        OutputFormat::Mp3 => Ok((
            encode_mp3(samples, sample_rate, DEFAULT_MP3_KBPS)?,
            OutputFormat::Mp3.content_type(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_metadata() {
        assert_eq!(OutputFormat::Wav.content_type(), "audio/wav");
        assert_eq!(OutputFormat::Mp3.content_type(), "audio/mpeg");
        assert_eq!(OutputFormat::Wav.extension(), "wav");
        assert_eq!(OutputFormat::Mp3.extension(), "mp3");
        assert_eq!(OutputFormat::parse("mp3"), Some(OutputFormat::Mp3));
        assert_eq!(OutputFormat::parse("WAV"), Some(OutputFormat::Wav));
        assert_eq!(OutputFormat::parse("flac"), None);
    }

    #[test]
    fn encode_mp3_produces_valid_frames() {
        let sr = 48_000u32;
        let samples: Vec<f32> = (0..sr / 10)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / sr as f32).sin() * 0.3)
            .collect();
        let bytes = encode_mp3(&samples, sr, 192).expect("encode mp3");
        assert!(
            bytes.len() > 100,
            "mp3 unexpectedly small: {} bytes",
            bytes.len()
        );
        // A valid MP3 stream starts with an ID3 tag or an MPEG frame sync
        // (0xFF followed by three set sync bits).
        let has_id3 = bytes.starts_with(b"ID3");
        let has_sync = bytes
            .windows(2)
            .any(|w| w[0] == 0xFF && (w[1] & 0xE0) == 0xE0);
        assert!(has_id3 || has_sync, "no MP3 frame sync / ID3 header found");
    }

    #[test]
    fn encode_dispatches_by_format() {
        let samples = vec![0.0f32; 4_800];
        let (wav, ct) = encode(&samples, 48_000, OutputFormat::Wav).unwrap();
        assert_eq!(ct, "audio/wav");
        assert_eq!(&wav[0..4], b"RIFF");

        let (mp3, ct2) = encode(&samples, 48_000, OutputFormat::Mp3).unwrap();
        assert_eq!(ct2, "audio/mpeg");
        assert!(mp3.len() > 50);
    }
}
