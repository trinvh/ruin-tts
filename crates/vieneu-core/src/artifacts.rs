//! Resolve model artifacts from the Hugging Face cache (downloading on demand)
//! or from a local directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const V3_REPO: &str = "pnnbao-ump/VieNeu-TTS-v3-Turbo";
pub const CODEC_REPO: &str = "OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano-ONNX";

/// Files that must be present so ONNX Runtime can resolve external data.
const V3_FILES: &[&str] = &[
    "onnx/vieneu_prefill.onnx",
    "onnx/vieneu_decode_step.onnx",
    "onnx/vieneu_acoustic_cached.onnx",
    "onnx/vieneu_backbone_shared.data",
    "onnx/vieneu_v3_heads.npz",
    "config.json",
    "tokenizer.json",
];
const CODEC_FILES: &[&str] = &[
    "moss_audio_tokenizer_decode_full.onnx",
    "moss_audio_tokenizer_decode_shared.data",
    "moss_audio_tokenizer_encode.onnx",
    "moss_audio_tokenizer_encode.data",
];

/// Filesystem locations of every artifact the engine loads.
#[derive(Debug, Clone)]
pub struct Artifacts {
    pub prefill: PathBuf,
    pub decode_step: PathBuf,
    pub acoustic: PathBuf,
    pub heads_npz: PathBuf,
    pub config: PathBuf,
    pub tokenizer: PathBuf,
    pub codec_decode: PathBuf,
    pub codec_encode: PathBuf,
}

/// Where to source the model from.
#[derive(Debug, Clone, Default)]
pub enum ModelSource {
    /// Fetch from the Hugging Face hub (cached after first download).
    #[default]
    Hub,
    /// Use a local directory laid out like the HF repo (must contain `onnx/`,
    /// `config.json`, `tokenizer.json`) plus a codec directory.
    Local { v3_dir: PathBuf, codec_dir: PathBuf },
}

impl Artifacts {
    pub fn resolve(source: &ModelSource, hf_token: Option<&str>) -> Result<Self> {
        match source {
            ModelSource::Hub => resolve_from_hub(hf_token),
            ModelSource::Local { v3_dir, codec_dir } => Ok(resolve_local(v3_dir, codec_dir)),
        }
    }
}

fn resolve_from_hub(hf_token: Option<&str>) -> Result<Artifacts> {
    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new()
        .with_token(hf_token.map(|s| s.to_string()))
        .build()
        .context("build HF api")?;

    let v3 = api.model(V3_REPO.to_string());
    let mut v3_paths = std::collections::HashMap::new();
    for f in V3_FILES {
        let p = v3.get(f).with_context(|| format!("fetch {V3_REPO}/{f}"))?;
        v3_paths.insert(*f, p);
    }

    let codec = api.model(CODEC_REPO.to_string());
    let mut codec_paths = std::collections::HashMap::new();
    for f in CODEC_FILES {
        let p = codec
            .get(f)
            .with_context(|| format!("fetch {CODEC_REPO}/{f}"))?;
        codec_paths.insert(*f, p);
    }

    Ok(Artifacts {
        prefill: v3_paths["onnx/vieneu_prefill.onnx"].clone(),
        decode_step: v3_paths["onnx/vieneu_decode_step.onnx"].clone(),
        acoustic: v3_paths["onnx/vieneu_acoustic_cached.onnx"].clone(),
        heads_npz: v3_paths["onnx/vieneu_v3_heads.npz"].clone(),
        config: v3_paths["config.json"].clone(),
        tokenizer: v3_paths["tokenizer.json"].clone(),
        codec_decode: codec_paths["moss_audio_tokenizer_decode_full.onnx"].clone(),
        codec_encode: codec_paths["moss_audio_tokenizer_encode.onnx"].clone(),
    })
}

fn resolve_local(v3_dir: &Path, codec_dir: &Path) -> Artifacts {
    let onnx = v3_dir.join("onnx");
    Artifacts {
        prefill: onnx.join("vieneu_prefill.onnx"),
        decode_step: onnx.join("vieneu_decode_step.onnx"),
        acoustic: onnx.join("vieneu_acoustic_cached.onnx"),
        heads_npz: onnx.join("vieneu_v3_heads.npz"),
        config: v3_dir.join("config.json"),
        tokenizer: v3_dir.join("tokenizer.json"),
        codec_decode: codec_dir.join("moss_audio_tokenizer_decode_full.onnx"),
        codec_encode: codec_dir.join("moss_audio_tokenizer_encode.onnx"),
    }
}
