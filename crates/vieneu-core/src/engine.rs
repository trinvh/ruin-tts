//! Torch-free v3-Turbo inference engine — Rust port of `onnx_runtime_lite.py`.
//!
//! Transformer forwards run in ONNX Runtime; embeddings, output heads, sampling
//! and prompt building are plain `ndarray`. One engine instance is single-owner
//! (not internally shared); the server runs a pool of them for parallelism.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Returned when a generation is cancelled cooperatively via the cancel flag.
#[derive(Debug, thiserror::Error)]
#[error("generation cancelled")]
pub struct Cancelled;

/// True if `err` is a [`Cancelled`] signal.
pub fn is_cancelled(err: &anyhow::Error) -> bool {
    err.downcast_ref::<Cancelled>().is_some()
}

use anyhow::{Context, Result};
use ndarray::{concatenate, s, Array1, Array2, Array3, Array4, ArrayD, Axis, Ix3, Ix4};
use ort::session::{builder::GraphOptimizationLevel, Session, SessionInputValue, SessionOutputs};
use ort::value::Tensor;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sea_g2p_rs::Pipeline;
use tokenizers::Tokenizer;

use crate::artifacts::{Artifacts, ModelSource};
use crate::config::ModelConfig;
use crate::npz::{self, Heads};
use crate::sampling::{sample, SamplingParams};
use crate::text;
use crate::voices::VoiceBook;

type Feeds = Vec<(Cow<'static, str>, SessionInputValue<'static>)>;

/// `ort::Error` (and the builder's `Error<SessionBuilder>`) are not
/// `Send + Sync`, so they can't flow into `anyhow` directly. Convert via
/// `Display` at the call site.
trait OrtAny<T> {
    fn any(self) -> Result<T>;
}
impl<T, E: std::fmt::Display> OrtAny<T> for std::result::Result<T, E> {
    fn any(self) -> Result<T> {
        self.map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

fn feed_f32(
    name: &'static str,
    arr: ArrayD<f32>,
) -> Result<(Cow<'static, str>, SessionInputValue<'static>)> {
    Ok((
        Cow::Borrowed(name),
        SessionInputValue::from(Tensor::from_array(arr).any()?),
    ))
}
fn feed_i64(
    name: &'static str,
    arr: ArrayD<i64>,
) -> Result<(Cow<'static, str>, SessionInputValue<'static>)> {
    Ok((
        Cow::Borrowed(name),
        SessionInputValue::from(Tensor::from_array(arr).any()?),
    ))
}
fn feed_i32(
    name: &'static str,
    arr: ArrayD<i32>,
) -> Result<(Cow<'static, str>, SessionInputValue<'static>)> {
    Ok((
        Cow::Borrowed(name),
        SessionInputValue::from(Tensor::from_array(arr).any()?),
    ))
}

/// How a voice is chosen for an utterance.
pub enum VoiceSelection {
    /// Use the model's default preset voice.
    Default,
    /// A built-in preset by name.
    Preset(String),
    /// Clone from a reference audio file.
    ClonePath(PathBuf),
    /// Clone from pre-encoded reference codes `(T, n_vq)`.
    CloneCodes(Array2<i64>),
}

/// Per-utterance synthesis options.
pub struct InferOptions {
    pub voice: VoiceSelection,
    pub emotion: String, // "natural" | "storytelling"
    pub sampling: SamplingParams,
    pub max_new_frames: usize,
    pub max_chars: usize,
    pub silence_p: f32,
    pub crossfade_p: f32,
}

impl Default for InferOptions {
    fn default() -> Self {
        Self {
            voice: VoiceSelection::Default,
            emotion: "natural".to_string(),
            sampling: SamplingParams::default(),
            max_new_frames: 300,
            max_chars: 256,
            silence_p: 0.15,
            crossfade_p: 0.0,
        }
    }
}

pub struct Engine {
    cfg: ModelConfig,
    heads: Heads,
    tokenizer: Tokenizer,
    pipe: Pipeline,
    voices: VoiceBook,

    sess_pre: Session,
    sess_dec: Session,
    sess_ac: Session,
    sess_codec_dec: Session,
    codec_encode_path: PathBuf,
    sess_codec_enc: Option<Session>,

    rng: StdRng,
    sample_rate: u32,
}

impl Engine {
    /// Load the engine. `threads` of 0 lets ORT pick; a fixed seed makes
    /// sampling reproducible.
    pub fn load(
        source: &ModelSource,
        hf_token: Option<&str>,
        threads: usize,
        seed: Option<u64>,
    ) -> Result<Self> {
        let art = Artifacts::resolve(source, hf_token)?;
        let cfg = ModelConfig::from_json_path(&art.config)?;
        let heads = npz::load_heads(&art.heads_npz)?;
        let tokenizer = Tokenizer::from_file(&art.tokenizer)
            .map_err(|e| anyhow::anyhow!("load tokenizer: {e}"))?;
        let pipe = Pipeline::new().context("init sea-g2p pipeline")?;
        let voices = VoiceBook::load_embedded()?;

        let sess_pre = build_session(&art.prefill, threads).context("load prefill")?;
        let sess_dec = build_session(&art.decode_step, threads).context("load decode_step")?;
        let sess_ac = build_session(&art.acoustic, threads).context("load acoustic")?;
        let sess_codec_dec =
            build_session(&art.codec_decode, threads).context("load codec decode")?;

        let rng = match seed {
            Some(s) => StdRng::seed_from_u64(s),
            None => StdRng::from_entropy(),
        };
        let sample_rate = cfg.audio_sample_rate;

        Ok(Self {
            cfg,
            heads,
            tokenizer,
            pipe,
            voices,
            sess_pre,
            sess_dec,
            sess_ac,
            sess_codec_dec,
            codec_encode_path: art.codec_encode,
            sess_codec_enc: None,
            rng,
            sample_rate,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn voices(&self) -> &VoiceBook {
        &self.voices
    }

    // ── Public synthesis ──────────────────────────────────────────────────

    /// Synthesize `text` into a mono f32 waveform at the model sample rate.
    pub fn infer(&mut self, text: &str, opts: &InferOptions) -> Result<Vec<f32>> {
        static NEVER: AtomicBool = AtomicBool::new(false);
        self.infer_cancellable(text, opts, &NEVER)
    }

    /// Like [`infer`], but checks `cancel` between chunks and frames and returns
    /// a [`Cancelled`] error if it is set.
    pub fn infer_cancellable(
        &mut self,
        text: &str,
        opts: &InferOptions,
        cancel: &AtomicBool,
    ) -> Result<Vec<f32>> {
        let (ref_codes, voice_token_id) = self.resolve_voice(&opts.voice)?;

        let chunks = text::split_text_into_chunks(text, opts.max_chars);
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let mut wavs: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            if cancel.load(Ordering::Relaxed) {
                return Err(Cancelled.into());
            }
            let phonemes = text::phonemize_with_emotions(&self.pipe, chunk);
            let wav = self.infer_phonemes(
                &phonemes,
                ref_codes.as_ref(),
                voice_token_id,
                &opts.emotion,
                &opts.sampling,
                opts.max_new_frames,
                cancel,
            )?;
            wavs.push(wav);
        }
        Ok(text::join_audio_chunks(
            &wavs,
            self.sample_rate,
            opts.silence_p,
            opts.crossfade_p,
        ))
    }

    fn resolve_voice(
        &mut self,
        sel: &VoiceSelection,
    ) -> Result<(Option<Array2<i64>>, Option<i64>)> {
        match sel {
            VoiceSelection::CloneCodes(c) => Ok((Some(c.clone()), None)),
            VoiceSelection::ClonePath(p) => Ok((Some(self.encode_reference(p)?), None)),
            VoiceSelection::Preset(name) => {
                let v = self
                    .voices
                    .get(name)
                    .ok_or_else(|| anyhow::anyhow!("voice '{name}' not found"))?;
                Ok((Some(v.codes.clone()), v.reserved_id))
            }
            VoiceSelection::Default => {
                let v = self
                    .voices
                    .default_voice()
                    .ok_or_else(|| anyhow::anyhow!("no default voice configured"))?;
                Ok((Some(v.codes.clone()), v.reserved_id))
            }
        }
    }

    fn infer_phonemes(
        &mut self,
        phonemes: &str,
        ref_codes: Option<&Array2<i64>>,
        voice_token_id: Option<i64>,
        emotion: &str,
        sp: &SamplingParams,
        max_new_frames: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<f32>> {
        let emo = leading_token(&self.cfg, emotion, voice_token_id);
        // Split disjoint field borrows so per-step session runs (which return
        // outputs borrowing the session) don't collide with head/RNG access.
        let Self {
            cfg,
            heads,
            tokenizer,
            sess_pre,
            sess_dec,
            sess_ac,
            sess_codec_dec,
            rng,
            ..
        } = self;

        let rows = build_rows(cfg, tokenizer, phonemes, ref_codes, emo)?;
        let prompt_embeds = embed_rows(cfg, heads, &rows); // (1, T, H)
        let t_prompt = prompt_embeds.shape()[1];
        let n_vq = cfg.n_vq;
        let layers = cfg.num_hidden_layers;

        // Prefill (scoped so `pre` is dropped before the decode loop).
        let (mut h, mut past_k, mut past_v) = {
            let pre = sess_pre
                .run(vec![feed_f32("inputs_embeds", prompt_embeds.into_dyn())?])
                .any()?;
            let h: Array1<f32> = {
                let hidden = pre["hidden"].try_extract_array::<f32>().any()?;
                hidden.slice(s![0, t_prompt - 1, ..]).to_owned()
            };
            let mut pk: Vec<Array4<f32>> = Vec::with_capacity(layers);
            let mut pv: Vec<Array4<f32>> = Vec::with_capacity(layers);
            for i in 0..layers {
                pk.push(extract4(&pre, &format!("present_k_{i}"))?);
                pv.push(extract4(&pre, &format!("present_v_{i}"))?);
            }
            (h, pk, pv)
        };

        let use_rep = (sp.repetition_penalty - 1.0).abs() > f32::EPSILON;
        let mut hist: Option<Vec<HashSet<usize>>> = if use_rep {
            Some(vec![HashSet::new(); n_vq])
        } else {
            None
        };

        let mut frames: Vec<Vec<i64>> = Vec::new();
        for t in 0..max_new_frames {
            if cancel.load(Ordering::Relaxed) {
                return Err(Cancelled.into());
            }
            let (codes, eos) = acoustic_frame(cfg, heads, sess_ac, rng, &h, sp, hist.as_mut())?;
            frames.push(codes.clone());
            if eos {
                break;
            }
            // Feed the generated frame back into the backbone.
            let mut slot = Array2::<i64>::from_elem((1, n_vq + 1), cfg.audio_pad_token_id);
            slot[[0, 0]] = cfg.speech_generation_start_token_id;
            for (ch, &c) in codes.iter().enumerate() {
                slot[[0, ch + 1]] = c;
            }
            let se = embed_rows(cfg, heads, &slot); // (1,1,H)
            let pos = Array2::<i64>::from_elem((1, 1), (t_prompt + t) as i64);

            let mut feeds: Feeds = Vec::with_capacity(2 + 2 * layers);
            feeds.push(feed_f32("inputs_embeds", se.into_dyn())?);
            feeds.push(feed_i64("position_ids", pos.into_dyn())?);
            for i in 0..layers {
                feeds.push(feed_f32(
                    layer_name("past_k_", i),
                    std::mem::take(&mut past_k[i]).into_dyn(),
                )?);
                feeds.push(feed_f32(
                    layer_name("past_v_", i),
                    std::mem::take(&mut past_v[i]).into_dyn(),
                )?);
            }
            let out = sess_dec.run(feeds).any()?;
            h = {
                let hid = out["hidden"].try_extract_array::<f32>().any()?;
                hid.slice(s![0, 0, ..]).to_owned()
            };
            for i in 0..layers {
                past_k[i] = extract4(&out, &format!("present_k_{i}"))?;
                past_v[i] = extract4(&out, &format!("present_v_{i}"))?;
            }
        }

        if frames.is_empty() {
            return Ok(Vec::new());
        }
        decode_codes(cfg, sess_codec_dec, &frames)
    }

    /// Encode a reference clip into MOSS codes `(T, n_vq)` for voice cloning.
    pub fn encode_reference(&mut self, path: &std::path::Path) -> Result<Array2<i64>> {
        let decoded = crate::audio::decode_file(path)?;
        let stereo = crate::audio::prepare_reference(&decoded, self.sample_rate);
        let n = stereo[0].len();
        let mut wav = Array3::<f32>::zeros((1, 2, n));
        for (c, ch) in stereo.iter().enumerate().take(2) {
            for (i, &sm) in ch.iter().enumerate() {
                wav[[0, c, i]] = sm;
            }
        }
        let lens = Array1::<i32>::from_elem(1, n as i32);

        if self.sess_codec_enc.is_none() {
            self.sess_codec_enc =
                Some(build_session(&self.codec_encode_path, 0).context("load codec encode")?);
        }
        let enc = self.sess_codec_enc.as_mut().unwrap();
        let feeds: Feeds = vec![
            feed_f32("waveform", wav.into_dyn())?,
            feed_i32("input_lengths", lens.into_dyn())?,
        ];
        let out = enc.run(feeds).any()?;
        let codes = out["audio_codes"].try_extract_array::<i32>().any()?; // (1, T, n_vq)
        let codes = codes.into_dimensionality::<Ix3>()?;
        let t = codes.shape()[1];
        let n_vq = codes.shape()[2];
        let mut res = Array2::<i64>::zeros((t, n_vq));
        for i in 0..t {
            for ch in 0..n_vq {
                res[[i, ch]] = codes[[0, i, ch]] as i64;
            }
        }
        Ok(res)
    }
}

// ── Free functions (explicit field refs → splittable borrows) ──────────────

fn build_session(path: &std::path::Path, threads: usize) -> Result<Session> {
    let mut b = Session::builder()
        .any()?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .any()?;
    if threads > 0 {
        b = b.with_intra_threads(threads).any()?;
    }
    b.commit_from_file(path).any()
}

fn leading_token(cfg: &ModelConfig, emotion: &str, voice_token_id: Option<i64>) -> i64 {
    if let Some(t) = voice_token_id {
        return t;
    }
    if emotion == "natural" {
        cfg.emotion_0_token_id
    } else {
        cfg.emotion_4_token_id
    }
}

fn build_rows(
    cfg: &ModelConfig,
    tokenizer: &Tokenizer,
    phonemes: &str,
    ref_codes: Option<&Array2<i64>>,
    emo_token: i64,
) -> Result<Array2<i64>> {
    let enc = tokenizer
        .encode(phonemes, false)
        .map_err(|e| anyhow::anyhow!("tokenize: {e}"))?;
    let n_vq = cfg.n_vq;
    let pad = cfg.audio_pad_token_id;

    let mut text_ids: Vec<i64> = Vec::with_capacity(enc.get_ids().len() + 3);
    text_ids.push(emo_token);
    text_ids.push(cfg.text_prompt_start_token_id);
    text_ids.extend(enc.get_ids().iter().map(|&id| id as i64));
    text_ids.push(cfg.text_prompt_end_token_id);

    let t = text_ids.len();
    let mut rows = Array2::<i64>::from_elem((t, n_vq + 1), pad);
    for (i, &id) in text_ids.iter().enumerate() {
        rows[[i, 0]] = id;
    }

    let Some(rc) = ref_codes else {
        return Ok(rows);
    };
    let r = rc.nrows();
    let mut refr = Array2::<i64>::from_elem((r, n_vq + 1), pad);
    for i in 0..r {
        refr[[i, 0]] = cfg.audio_ref_slot_token_id;
        for ch in 0..n_vq {
            refr[[i, ch + 1]] = rc[[i, ch]];
        }
    }
    Ok(concatenate(Axis(0), &[rows.view(), refr.view()])?)
}

/// rows: (T, n_vq+1) → (1, T, H) embeddings.
fn embed_rows(cfg: &ModelConfig, heads: &Heads, rows: &Array2<i64>) -> Array3<f32> {
    let n_vq = cfg.n_vq;
    let h = cfg.hidden_size;
    let pad = cfg.audio_pad_token_id;
    let t = rows.nrows();
    let mut emb = Array2::<f32>::zeros((t, h));
    for i in 0..t {
        let tid = rows[[i, 0]] as usize;
        let mut row = emb.row_mut(i);
        row += &heads.text_emb.row(tid);
        for ch in 0..n_vq {
            let id = rows[[i, ch + 1]];
            if id != pad {
                row += &heads.audio_emb.slice(s![ch, id as usize, ..]);
            }
        }
    }
    emb.insert_axis(Axis(0))
}

/// One acoustic frame: `n_vq` cached ONNX steps + numpy-style heads/sampling.
#[allow(clippy::too_many_arguments)]
fn acoustic_frame(
    cfg: &ModelConfig,
    heads: &Heads,
    sess_ac: &mut Session,
    rng: &mut StdRng,
    h: &Array1<f32>,
    sp: &SamplingParams,
    mut hist: Option<&mut Vec<HashSet<usize>>>,
) -> Result<(Vec<i64>, bool)> {
    let hsz = cfg.hidden_size;
    let n_head = cfg.local_num_attention_heads;
    let hd = cfg.local_head_dim();
    let n_vq = cfg.n_vq;
    let sgs = cfg.speech_generation_start_token_id as usize;

    let empty = || Array4::<f32>::zeros((1, n_head, 0, hd)).into_dyn();

    // Step 0: [cond, sgs-text-emb]
    let txt = heads.text_emb.row(sgs).to_owned();
    let mut tok = Array3::<f32>::zeros((1, 2, hsz));
    tok.slice_mut(s![0, 0, ..]).assign(h);
    tok.slice_mut(s![0, 1, ..]).assign(&txt);
    let pos = ndarray::array![[0i64, 1i64]];

    let feeds: Feeds = vec![
        feed_f32("token_emb", tok.into_dyn())?,
        feed_i64("position_ids", pos.into_dyn())?,
        feed_f32("past_k_0", empty())?,
        feed_f32("past_k_1", empty())?,
        feed_f32("past_v_0", empty())?,
        feed_f32("past_v_1", empty())?,
    ];
    let out = sess_ac.run(feeds).any()?;
    let (slot0, first_vec): (Array1<f32>, Array1<f32>) = {
        let hid = out["hidden"].try_extract_array::<f32>().any()?;
        (
            hid.slice(s![0, 0, ..]).to_owned(),
            hid.slice(s![0, 1, ..]).to_owned(),
        )
    };
    let mut pk0 = extract4(&out, "present_k_0")?;
    let mut pk1 = extract4(&out, "present_k_1")?;
    let mut pv0 = extract4(&out, "present_v_0")?;
    let mut pv1 = extract4(&out, "present_v_1")?;
    drop(out);

    let mut codes: Vec<i64> = Vec::with_capacity(n_vq);
    codes.push(sample_channel(
        heads,
        rng,
        0,
        &first_vec,
        sp,
        hist.as_deref_mut(),
    ));

    for ch in 1..n_vq {
        let prev_code = codes[ch - 1] as usize;
        let emb = heads.audio_emb.slice(s![ch - 1, prev_code, ..]).to_owned();
        let tok1 = emb.into_shape_with_order((1, 1, hsz))?;
        let pos1 = Array2::<i64>::from_elem((1, 1), (ch + 1) as i64);

        let feeds: Feeds = vec![
            feed_f32("token_emb", tok1.into_dyn())?,
            feed_i64("position_ids", pos1.into_dyn())?,
            feed_f32("past_k_0", std::mem::take(&mut pk0).into_dyn())?,
            feed_f32("past_k_1", std::mem::take(&mut pk1).into_dyn())?,
            feed_f32("past_v_0", std::mem::take(&mut pv0).into_dyn())?,
            feed_f32("past_v_1", std::mem::take(&mut pv1).into_dyn())?,
        ];
        let out = sess_ac.run(feeds).any()?;
        let vec_ch: Array1<f32> = {
            let hid = out["hidden"].try_extract_array::<f32>().any()?;
            hid.slice(s![0, 0, ..]).to_owned()
        };
        pk0 = extract4(&out, "present_k_0")?;
        pk1 = extract4(&out, "present_k_1")?;
        pv0 = extract4(&out, "present_v_0")?;
        pv1 = extract4(&out, "present_v_1")?;
        drop(out);
        codes.push(sample_channel(
            heads,
            rng,
            ch,
            &vec_ch,
            sp,
            hist.as_deref_mut(),
        ));
    }

    // EOS from the text head on slot 0.
    let text_logits = heads.text_emb.dot(&slot0);
    let eos = argmax1(&text_logits) == cfg.speech_generation_end_token_id as usize;
    Ok((codes, eos))
}

fn sample_channel(
    heads: &Heads,
    rng: &mut StdRng,
    ch: usize,
    vec_h: &Array1<f32>,
    sp: &SamplingParams,
    hist: Option<&mut Vec<HashSet<usize>>>,
) -> i64 {
    let logits = heads.audio_emb.slice(s![ch, .., ..]).dot(vec_h); // (Va,)
    let prev = hist.as_deref().map(|h| &h[ch]);
    let code = sample(logits.as_slice().unwrap(), sp, prev, rng);
    if let Some(h) = hist {
        h[ch].insert(code);
    }
    code as i64
}

fn decode_codes(cfg: &ModelConfig, sess: &mut Session, frames: &[Vec<i64>]) -> Result<Vec<f32>> {
    let t = frames.len();
    let n_vq = cfg.n_vq;
    let mut codes = Array3::<i32>::zeros((1, t, n_vq));
    for (i, frame) in frames.iter().enumerate() {
        for (ch, &c) in frame.iter().enumerate() {
            codes[[0, i, ch]] = c as i32;
        }
    }
    let lens = Array1::<i32>::from_elem(1, t as i32);
    let feeds: Feeds = vec![
        feed_i32("audio_codes", codes.into_dyn())?,
        feed_i32("audio_code_lengths", lens.into_dyn())?,
    ];
    let out = sess.run(feeds).any()?;
    let mono = {
        let audio = out["audio"].try_extract_array::<f32>().any()?; // (1, C, S)
        let audio = audio.into_dimensionality::<Ix3>()?;
        audio.slice(s![0, .., ..]).mean_axis(Axis(0)).unwrap()
    };
    Ok(mono.to_vec())
}

// ── small helpers ──────────────────────────────────────────────────────────

fn layer_name(prefix: &str, i: usize) -> &'static str {
    // ORT input names must be `'static`; intern the 24 KV names once.
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Lazy<Mutex<HashMap<String, &'static str>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    let key = format!("{prefix}{i}");
    let mut m = CACHE.lock().unwrap();
    if let Some(s) = m.get(&key) {
        return s;
    }
    let leaked: &'static str = Box::leak(key.clone().into_boxed_str());
    m.insert(key, leaked);
    leaked
}

fn extract4(outputs: &SessionOutputs, name: &str) -> Result<Array4<f32>> {
    let view = outputs[name].try_extract_array::<f32>().any()?;
    Ok(view.into_dimensionality::<Ix4>()?.to_owned())
}

fn argmax1(x: &Array1<f32>) -> usize {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in x.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best
}
