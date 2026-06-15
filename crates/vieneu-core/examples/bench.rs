//! Generation-time benchmark (excludes model load). Greedy decoding so the work
//! is identical run-to-run and matches the Python reference.
//!
//!   cargo run -p vieneu-core --example bench --release -- [iters] [voice]

use std::time::Instant;
use vieneu_core::{Engine, InferOptions, ModelSource, SamplingParams, VoiceSelection};

const TEXT: &str =
    "Hôm nay trời đẹp, tôi quyết định đi dạo quanh hồ và ngắm những hàng cây xanh mướt bên đường.";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let iters: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    let voice = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "Bình An".to_string());

    let mut engine = Engine::load(&ModelSource::Hub, None, 0, Some(0))?;
    let opts = InferOptions {
        voice: VoiceSelection::Preset(voice),
        sampling: SamplingParams {
            temperature: 0.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // warmup
    let wav = engine.infer(TEXT, &opts)?;
    let audio_s = wav.len() as f32 / engine.sample_rate() as f32;

    let mut times = Vec::new();
    for _ in 0..iters {
        let t = Instant::now();
        let _ = engine.infer(TEXT, &opts)?;
        times.push(t.elapsed().as_secs_f32());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let best = times[0];
    let med = times[times.len() / 2];
    println!(
        "RUST  audio={audio_s:.2}s  gen median={med:.3}s best={best:.3}s  RTF={:.2}x (median)",
        audio_s / med
    );
    Ok(())
}
