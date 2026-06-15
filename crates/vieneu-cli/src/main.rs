//! vieneu — CLI for batch novel→audio synthesis (offline, no server).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use vieneu_core::{
    Engine, InferOptions, ModelSource, OutputFormat, SamplingParams, VoiceSelection,
};

#[derive(Parser)]
#[command(name = "vieneu", about = "VieNeu-TTS v3-Turbo CLI (Rust)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List built-in preset voices.
    Voices,
    /// Synthesize a single string or text file.
    Synth(SynthArgs),
    /// Synthesize every `*.txt` in a directory into matching `*.wav` files.
    Batch(BatchArgs),
}

#[derive(Parser)]
struct SynthArgs {
    /// Text to speak (or use --file).
    #[arg(long)]
    text: Option<String>,
    /// Read text from a file.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Output WAV path.
    #[arg(long, default_value = "out.wav")]
    out: PathBuf,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Parser)]
struct BatchArgs {
    /// Directory of `*.txt` chapter files.
    #[arg(long)]
    input_dir: PathBuf,
    /// Directory to write `*.wav` files into.
    #[arg(long)]
    out_dir: PathBuf,
    /// Parallel engine workers (defaults to a sensible value for this machine).
    #[arg(long)]
    workers: Option<usize>,
    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Parser, Clone)]
struct CommonArgs {
    /// Preset voice id.
    #[arg(long)]
    voice: Option<String>,
    /// "natural" or "storytelling".
    #[arg(long, default_value = "natural")]
    emotion: String,
    #[arg(long, default_value_t = 0.8)]
    temperature: f32,
    #[arg(long, default_value_t = 25)]
    top_k: usize,
    #[arg(long, default_value_t = 0.95)]
    top_p: f32,
    #[arg(long, default_value_t = 1.2)]
    repetition_penalty: f32,
    #[arg(long, default_value_t = 256)]
    max_chars: usize,
    /// Output format: "wav" or "mp3".
    #[arg(long, default_value = "wav")]
    format: String,
    /// Fixed RNG seed for reproducible narration.
    #[arg(long)]
    seed: Option<u64>,
    /// Local model dir (HF layout); requires --codec-dir. Default: download.
    #[arg(long)]
    model_dir: Option<String>,
    #[arg(long)]
    codec_dir: Option<String>,
}

impl CommonArgs {
    fn source(&self) -> Result<ModelSource> {
        match (&self.model_dir, &self.codec_dir) {
            (Some(v), Some(c)) => Ok(ModelSource::Local {
                v3_dir: v.into(),
                codec_dir: c.into(),
            }),
            (Some(_), None) => anyhow::bail!("--model-dir requires --codec-dir"),
            _ => Ok(ModelSource::Hub),
        }
    }

    fn options(&self) -> InferOptions {
        let voice = self
            .voice
            .clone()
            .map(VoiceSelection::Preset)
            .unwrap_or(VoiceSelection::Default);
        InferOptions {
            voice,
            emotion: self.emotion.clone(),
            sampling: SamplingParams {
                temperature: self.temperature,
                top_k: self.top_k,
                top_p: self.top_p,
                repetition_penalty: self.repetition_penalty,
            },
            max_chars: self.max_chars,
            ..Default::default()
        }
    }

    fn output_format(&self) -> Result<OutputFormat> {
        OutputFormat::parse(&self.format).ok_or_else(|| {
            anyhow::anyhow!("unsupported --format '{}' (use wav or mp3)", self.format)
        })
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "vieneu=info,ort=warn".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Voices => list_voices(),
        Cmd::Synth(a) => synth(a),
        Cmd::Batch(a) => batch(a),
    }
}

fn list_voices() -> Result<()> {
    let engine = Engine::load(&ModelSource::Hub, None, 0, None)?;
    for (label, id) in engine.voices().list() {
        println!("{id}\t{label}");
    }
    Ok(())
}

fn synth(a: SynthArgs) -> Result<()> {
    let text = match (&a.text, &a.file) {
        (Some(t), _) => t.clone(),
        (None, Some(f)) => {
            std::fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?
        }
        (None, None) => anyhow::bail!("provide --text or --file"),
    };
    let format = a.common.output_format()?;
    let mut engine = Engine::load(&a.common.source()?, None, 0, a.common.seed)?;
    let wav = engine.infer(&text, &a.common.options())?;
    let (bytes, _) = vieneu_core::encode(&wav, engine.sample_rate(), format)?;
    std::fs::write(&a.out, &bytes).with_context(|| format!("write {}", a.out.display()))?;
    let dur = wav.len() as f32 / engine.sample_rate() as f32;
    println!("wrote {} ({:.1}s)", a.out.display(), dur);
    Ok(())
}

fn batch(a: BatchArgs) -> Result<()> {
    std::fs::create_dir_all(&a.out_dir)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&a.input_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .collect();
    files.sort();
    if files.is_empty() {
        anyhow::bail!("no .txt files in {}", a.input_dir.display());
    }

    let default_workers = std::thread::available_parallelism()
        .map(|n| (n.get() / 3).max(1))
        .unwrap_or(2);
    let workers = a.workers.unwrap_or(default_workers).min(files.len());
    let format = a.common.output_format()?;
    println!("{} chapter(s), {} worker(s)", files.len(), workers);

    let queue = Arc::new(Mutex::new(
        files.into_iter().collect::<std::collections::VecDeque<_>>(),
    ));
    let source = a.common.source()?;
    let opts = Arc::new(a.common.clone());
    let out_dir = a.out_dir.clone();

    let mut handles = Vec::new();
    for w in 0..workers {
        let queue = queue.clone();
        let source = source.clone();
        let opts = opts.clone();
        let out_dir = out_dir.clone();
        handles.push(std::thread::spawn(move || -> Result<()> {
            let mut engine = Engine::load(&source, None, 0, opts.seed)?;
            loop {
                let next = queue.lock().unwrap().pop_front();
                let Some(path) = next else { break };
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?;
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let out = out_dir.join(format!("{stem}.{}", format.extension()));
                let t0 = std::time::Instant::now();
                let wav = engine.infer(&text, &opts.options())?;
                let (bytes, _) = vieneu_core::encode(&wav, engine.sample_rate(), format)?;
                std::fs::write(&out, &bytes).with_context(|| format!("write {}", out.display()))?;
                let dur = wav.len() as f32 / engine.sample_rate() as f32;
                println!(
                    "[w{w}] {} → {} ({:.1}s audio in {:.1}s)",
                    path.file_name().unwrap().to_string_lossy(),
                    out.display(),
                    dur,
                    t0.elapsed().as_secs_f32()
                );
            }
            Ok(())
        }));
    }

    for h in handles {
        h.join()
            .map_err(|_| anyhow::anyhow!("worker thread panicked"))??;
    }
    println!("done.");
    Ok(())
}
