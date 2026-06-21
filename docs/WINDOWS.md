# Building & running on Windows

The whole stack runs on Windows (x64). It was developed on macOS, so the
`Makefile` targets are Unix-only — on Windows run the underlying `cargo` /
`pnpm` / `uvicorn` commands directly, as shown below.

Platform notes baked into the code:
- The Tauri shell resolves the sidecar binaries with the right `.exe` suffix.
- TTS (`vieneu-core`) uses ONNX Runtime on the **CPU** provider — no CoreML/Metal —
  and `ort` downloads the Windows ONNX Runtime automatically at build time.
- The dubbing ASR sidecar swaps Apple's `mlx-whisper` for `openai-whisper` off
  macOS automatically (`sys_platform` markers + a runtime branch in `asr.py`);
  diarization picks CUDA → CPU when MPS is absent.
- All paths use `PathBuf` / the `dirs` crate / the system temp dir.

## Prerequisites

1. **Rust** (MSVC toolchain): install via <https://rustup.rs>; default host
   `x86_64-pc-windows-msvc`.
2. **Visual Studio Build Tools** with the *Desktop development with C++* workload
   (needed by `ort`, `tokenizers/onig`, `sqlite`).
3. **WebView2 runtime** — preinstalled on Windows 10/11 (required by Tauri).
4. **Node + pnpm**: `npm i -g pnpm` (the `ui/` frontend uses pnpm; do not use npm).
5. **ffmpeg / ffprobe** on `PATH` (export + media probing). Use a full build with
   `libass` for burned-in subtitles — e.g. the gyan.dev or BtbN builds. Without
   libass the export still works but ships *soft* (selectable) subtitles.
6. **Python 3.10+** — only for the video-dubbing sidecar (`services/media-ai`).

## Build the Rust servers

```powershell
cargo build --release -p vieneu-server          # TTS server  (:8080)
cargo build --release -p studio --bin studio-server  # studio server (:8090)
```

The first `vieneu-server` run downloads the VieNeu-TTS ONNX models from the
Hugging Face hub into the system temp dir (needs internet; set `HF_TOKEN` if the
repo is gated).

## Run the desktop app

```powershell
cd ui
pnpm install
pnpm tauri dev      # or: pnpm tauri build  (produces an .msi / .exe installer)
```

The Tauri shell launches `target\release\vieneu-server.exe` and
`target\release\studio-server.exe` if they exist (build them first, above).
Otherwise start them manually in separate terminals:

```powershell
.\target\release\vieneu-server.exe --addr 127.0.0.1:8080 --workers 2
.\target\release\studio-server.exe --addr 127.0.0.1:8090
```

You can also point the app at custom binaries with the `VIENEU_SERVER_BIN` /
`STUDIO_SERVER_BIN` environment variables.

## Video-dubbing sidecar (optional — only for the "Lồng tiếng" feature)

```powershell
cd services\media-ai
python -m venv .venv
.\.venv\Scripts\activate
pip install -e .            # installs openai-whisper (CPU) on Windows, not mlx
copy .env.example .env      # then put your HF_TOKEN in .env (for pyannote)
uvicorn app:app --port 8099
```

Notes:
- Whisper runs on the CPU by default and is slower than the macOS MLX path. For
  GPU, install a CUDA build of PyTorch first
  (<https://pytorch.org/get-started/locally/>) — diarization will use it.
- `openai-whisper` also shells out to `ffmpeg`, so the PATH requirement above
  applies here too.

## Secrets

The Ruin API key, Gemini key and YouTube credentials are entered in the app's
**Settings** page and stored in the runtime SQLite DB (`studio.db`) — never in
source or env. `*.db` is gitignored.
