# ruin-tts — VieNeu-TTS v3-Turbo in Rust

A native-Rust port of [VieNeu-TTS](https://github.com/pnnbao97/VieNeu-TTS) v3-Turbo
(the torch-free **ONNX** backend), optimized for Apple Silicon. Text → 48 kHz
Vietnamese/bilingual speech with built-in voices, instant voice cloning, and
inline emotion cues — fully on-device, no Python at runtime.

Built for narrating novels into audio for YouTube: an HTTP API, a batch CLI, and
a Tauri desktop demo app.

The Rust engine is **numerically validated** against the original Python ONNX
engine: greedy output correlates 1.0 over every sample. On a Mac mini M4 it runs
at roughly **4–5× realtime** on CPU.

## Architecture

A Cargo workspace (`crates/`) plus a Tauri app (`ui/`):

| Crate | Purpose |
|---|---|
| `sea-g2p-rs` | Vendored, PyO3-free fork of [sea-g2p](https://github.com/pnnbao97/sea-g2p) (Apache-2.0). Vietnamese normalization + grapheme→phoneme. The 48 MB dictionary is embedded. |
| `vieneu-core` | The engine: sea-g2p → HF `tokenizers` → ONNX Runtime (prefill / decode / acoustic) → MOSS neural codec → 48 kHz WAV. Preset voices, cloning, emotion cues, chunking. |
| `vieneu-server` | `axum` HTTP API with a worker pool of engines for parallel synthesis. |
| `vieneu-cli` | `vieneu` — single-shot and parallel batch (chapters → WAV). |
| `ui/` | Tauri 2 + React desktop app that spawns the server and calls its HTTP API. |

The model weights are reused unchanged — only the inference harness is ported.
Artifacts download automatically from Hugging Face on first run and are cached
(`pnnbao-ump/VieNeu-TTS-v3-Turbo` + `OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano-ONNX`).

## Prerequisites

- Rust (stable) — built/tested on 1.96.
- For the Tauri app: Node + pnpm, and Xcode Command Line Tools.
- A C compiler (clang, included with Xcode CLT) — used to build the bundled
  LAME MP3 encoder. No ffmpeg or other external tools required.
- ONNX Runtime is fetched automatically by the `ort` crate.

## Build

```bash
cargo build --release            # builds core, server, cli
```

The binaries embed the phoneme dictionary, so they are self-contained (~80 MB).

## CLI

```bash
# list built-in voices
./target/release/vieneu voices

# one clip
./target/release/vieneu synth \
  --text "Xin chào Việt Nam" --voice "Xuân Vĩnh" --out hello.wav

# whole book: every chapters/*.txt -> audio/*.wav, in parallel
./target/release/vieneu batch \
  --input-dir chapters --out-dir audio --voice "Ngọc Linh" --workers 4
```

Useful flags: `--format wav|mp3` (MP3 is 192 kbps, YouTube-ready),
`--emotion natural|storytelling`, `--temperature`, `--top-k`, `--top-p`,
`--repetition-penalty`, `--seed` (reproducible narration).

## HTTP API

```bash
./target/release/vieneu-server --addr 127.0.0.1:8080 --workers 2
```

| Method & path | Description |
|---|---|
| `GET /health` | liveness |
| `GET /v1/info` | sample rate, pool size, voice count |
| `GET /v1/voices` | `[{ id, label }]` |
| `POST /v1/tts` | JSON in → `audio/wav` out (synchronous) |
| `POST /v1/clone` | multipart `file` → `{ ref_id, frames }` |
| `POST /v1/jobs` | submit a long job → `{ job_id }` |
| `GET /v1/jobs/:id` | job status |
| `GET /v1/jobs/:id/download` | finished WAV |

Example:

```bash
curl -X POST http://127.0.0.1:8080/v1/tts \
  -H 'content-type: application/json' \
  -d '{"text":"[cười] Nghe hay quá đi.","voice":"Bình An","temperature":0.8}' \
  -o out.wav
```

`POST /v1/tts` / `/v1/jobs` body fields: `text` (required), `voice`, `ref_id`,
`emotion`, `temperature`, `top_k`, `top_p`, `repetition_penalty`, `max_chars`,
`max_new_frames`, `silence_p`, `crossfade_p`, `format` (`"wav"` default or
`"mp3"`). MP3 is encoded natively via LAME — no ffmpeg dependency.

## Desktop app (Tauri demo)

```bash
cargo build --release -p vieneu-server   # the app spawns this binary
pnpm -C ui install
pnpm -C ui tauri dev
```

The app launches the server automatically, lists voices, and calls the HTTP API
to synthesize, clone, and play audio. Override the server binary location with
`VIENEU_SERVER_BIN` if needed.

## Emotion cues

Drop these inline in the text (experimental): `[cười]` (chuckle),
`[thở dài]` (sigh), `[hắng giọng]` (clear throat).

## Scope

This port targets the v3-Turbo **ONNX/CPU** path only — the fastest option on
Apple Silicon. The original project's CUDA-only (LMDeploy) and GGUF backends are
intentionally omitted; v3-Turbo is a superset of their user-facing features.

## License

Apache-2.0, matching the upstream VieNeu-TTS and sea-g2p projects.
