//! Parallel throughput: N engines each rendering a clip concurrently, vs the
//! same N rendered sequentially. Shows the multi-core scaling a worker pool buys
//! (Python would need separate processes for this).
//!
//!   cargo run -p vieneu-core --example bench_par --release -- [N] [threads_per_engine]

use std::time::Instant;
use vieneu_core::{Engine, InferOptions, ModelSource, SamplingParams, VoiceSelection};

const TEXT: &str =
    "Hôm nay trời đẹp, tôi quyết định đi dạo quanh hồ và ngắm những hàng cây xanh mướt bên đường.";

fn opts() -> InferOptions {
    InferOptions {
        voice: VoiceSelection::Preset("Bình An".into()),
        sampling: SamplingParams {
            temperature: 0.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let threads: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2);

    eprintln!("loading {n} engines ({threads} intra-op threads each)…");
    let mut engines: Vec<Engine> = (0..n)
        .map(|_| Engine::load(&ModelSource::Hub, None, threads, Some(0)))
        .collect::<anyhow::Result<_>>()?;

    // warmup + measure single-clip audio length
    let wav = engines[0].infer(TEXT, &opts())?;
    let audio_s = wav.len() as f32 / engines[0].sample_rate() as f32;

    // Sequential: one engine renders N clips back to back.
    let t = Instant::now();
    for _ in 0..n {
        let _ = engines[0].infer(TEXT, &opts())?;
    }
    let seq = t.elapsed().as_secs_f32();

    // Parallel: N engines each render one clip at once.
    let t = Instant::now();
    std::thread::scope(|s| {
        for e in engines.iter_mut() {
            s.spawn(|| {
                let _ = e.infer(TEXT, &opts());
            });
        }
    });
    let par = t.elapsed().as_secs_f32();

    println!("N={n} clips of {audio_s:.2}s each");
    println!(
        "  sequential (1 engine): {seq:.2}s  → {:.2}x realtime",
        (audio_s * n as f32) / seq
    );
    println!(
        "  parallel  ({n} engines): {par:.2}s  → {:.2}x realtime",
        (audio_s * n as f32) / par
    );
    println!("  speedup: {:.2}x", seq / par);
    Ok(())
}
