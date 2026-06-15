//! End-to-end smoke test: load the v3-Turbo ONNX model and synthesize a clip.
//!
//!   cargo run -p vieneu-core --example smoke --release -- "Xin chào Việt Nam" out.wav [voice]
//!
//! Uses greedy decoding (temperature 0) for a deterministic first check.

use vieneu_core::{Engine, InferOptions, ModelSource, SamplingParams, VoiceSelection};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .try_init()
        .ok();

    let args: Vec<String> = std::env::args().collect();
    let text = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "Xin chào, đây là giọng nói tiếng Việt được tạo bằng Rust.".to_string());
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "out.wav".to_string());
    let voice = args.get(3).cloned();

    eprintln!("Loading engine (first run downloads ~hundreds of MB)…");
    let t0 = std::time::Instant::now();
    let mut engine = Engine::load(&ModelSource::Hub, None, 0, Some(0))?;
    eprintln!("Loaded in {:.1}s", t0.elapsed().as_secs_f32());

    eprintln!("Available voices:");
    for (label, id) in engine.voices().list() {
        eprintln!("  - {label}  [{id}]");
    }

    let opts = InferOptions {
        voice: voice
            .map(VoiceSelection::Preset)
            .unwrap_or(VoiceSelection::Default),
        // Greedy first for a deterministic, reproducible smoke check.
        sampling: SamplingParams {
            temperature: 0.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let t1 = std::time::Instant::now();
    let wav = engine.infer(&text, &opts)?;
    let dur = wav.len() as f32 / engine.sample_rate() as f32;
    let gen = t1.elapsed().as_secs_f32();
    eprintln!(
        "Generated {:.2}s of audio in {:.2}s ({:.2}x realtime), {} samples @ {} Hz",
        dur,
        gen,
        dur / gen,
        wav.len(),
        engine.sample_rate()
    );

    vieneu_core::audio::write_wav(std::path::Path::new(&out), &wav, engine.sample_rate())?;
    eprintln!("Wrote {out}");
    Ok(())
}
