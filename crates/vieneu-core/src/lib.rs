//! vieneu-core — torch-free Rust port of VieNeu-TTS v3 Turbo (ONNX) for Apple
//! Silicon. Text → 48 kHz Vietnamese/bilingual speech with preset voices,
//! instant voice cloning, and inline emotion cues.
//!
//! ```no_run
//! use vieneu_core::{Engine, InferOptions, VoiceSelection, ModelSource};
//! let mut engine = Engine::load(&ModelSource::Hub, None, 0, None)?;
//! let opts = InferOptions { voice: VoiceSelection::Preset("Xuân Vĩnh".into()), ..Default::default() };
//! let wav = engine.infer("Xin chào Việt Nam", &opts)?;
//! vieneu_core::audio::write_wav(std::path::Path::new("out.wav"), &wav, engine.sample_rate())?;
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod artifacts;
pub mod audio;
pub mod config;
pub mod encode;
pub mod engine;
pub mod npz;
pub mod sampling;
pub mod text;
pub mod voices;

pub use artifacts::ModelSource;
pub use config::ModelConfig;
pub use encode::{encode, encode_mp3, OutputFormat};
pub use engine::{is_cancelled, Cancelled, Engine, InferOptions, VoiceSelection};
pub use sampling::SamplingParams;
pub use voices::{PresetVoice, VoiceBook};
