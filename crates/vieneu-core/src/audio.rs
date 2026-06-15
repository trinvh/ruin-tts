//! Audio I/O: decode reference clips (any format via symphonia), resample, and
//! write/encode 48 kHz mono output (WAV via hound).

use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded audio: per-channel f32 samples plus the source sample rate.
pub struct DecodedAudio {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
}

/// Decode an audio file (wav/mp3/m4a/flac/…) into planar f32 channels.
pub fn decode_file(path: &Path) -> Result<DecodedAudio> {
    let file =
        std::fs::File::open(path).with_context(|| format!("open ref audio {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("probe audio format")?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default audio track"))?;
    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("unknown sample rate"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("make decoder")?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(e) => return Err(e).context("read packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e).context("decode packet"),
        };
        let spec = *decoded.spec();
        let n_ch = spec.channels.count();
        if channels.is_empty() {
            channels = vec![Vec::new(); n_ch];
        }
        if sample_buf.is_none() {
            sample_buf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        let inter = buf.samples();
        // deinterleave
        for (i, &s) in inter.iter().enumerate() {
            channels[i % n_ch].push(s);
        }
    }

    if channels.is_empty() || channels[0].is_empty() {
        bail!("decoded zero audio samples");
    }
    Ok(DecodedAudio {
        channels,
        sample_rate,
    })
}

/// Linear-interpolation resampler. Adequate for preparing a reference clip
/// before the neural codec re-encodes it; not intended for output-path use.
pub fn resample_linear(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = samples[idx.min(samples.len() - 1)];
        let b = samples[(idx + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

/// Prepare a reference clip for the MOSS encoder: resample to `target_sr` and
/// return exactly two channels (mono is duplicated; >2 channels are truncated).
pub fn prepare_reference(audio: &DecodedAudio, target_sr: u32) -> Vec<Vec<f32>> {
    let resampled: Vec<Vec<f32>> = audio
        .channels
        .iter()
        .map(|ch| resample_linear(ch, audio.sample_rate, target_sr))
        .collect();
    match resampled.len() {
        1 => vec![resampled[0].clone(), resampled[0].clone()],
        _ => vec![resampled[0].clone(), resampled[1].clone()],
    }
}

/// Encode a mono f32 waveform as a 16-bit PCM WAV byte buffer.
pub fn wav_bytes_i16(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(v)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Write a mono f32 waveform to a 16-bit PCM WAV file.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let bytes = wav_bytes_i16(samples, sample_rate)?;
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
