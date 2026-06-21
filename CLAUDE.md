# ruin-tts — project notes for Claude

## Running servers — DO NOT start them yourself
The user **always runs the servers themselves** (`vieneu-server`, `studio-server`
and the `media-ai` dubbing sidecar — each now binds a free port the desktop shell
picks at runtime — and the Tauri app via `make dev`). Do **not** start, restart,
or kill these processes. If you need a server for a quick check, use a throwaway port (e.g.
:8098) and a temp `--db`/`--work-dir`, then kill only that process. Never touch a
`studio-server`/`vieneu-server` launched from `ui/src-tauri/...` — that one
belongs to the user's app.

After changing Rust code that the Tauri app runs as a sidecar, the user must
restart their app to pick it up. The Tauri shell resolves sidecars from
`target/release/<name>`, so a **release** build is required:
`cargo build --release -p studio --bin studio-server` (and `vieneu-server` if it
changed). Just tell the user to restart — don't run it for them.

## Toolchain
- The `ui/` frontend uses **pnpm** (there is a `pnpm-lock.yaml`). Do **not** use
  npm — it errors on the lockfile.
- Rust: `cargo test -p studio`, `cargo fmt`, `cargo clippy`.

## Video dubbing (foreign video → Vietnamese)
- The "Lồng tiếng" page imports a video and runs a 6-step human-in-the-loop
  pipeline: extract audio → analyze → translate → synthesize → build track →
  export. It is **separate** from the audiobook node-graph engine (its own
  `dub_*` tables + `/api/dub/*` endpoints in `crates/studio/src/dub/`).
- Heavy ML lives in a **Python sidecar** at `services/media-ai/` (FastAPI on
  :8099): mlx-whisper (ASR) + pyannote community-1 (diarization) + audeering
  wav2vec2 (age/gender). It is stateless and file-path based; studio shares its
  `work_dir`. The user runs it themselves like vieneu-server (`uvicorn app:app
  --port 8099` in `services/media-ai`, needs `HF_TOKEN` in its `.env`).
- Translation is **Gemini**, called from Rust (`dub/clients.rs`) so the key stays
  in the studio SQLite DB. Time-fit uses ffmpeg `atempo` (cap ~1.5×; segments
  flagged `long` can be "translated shorter").

## Secrets
- The Ruin API key and YouTube credentials live **only** in the runtime SQLite DB
  (edited via the app's Settings page), never in source or env. `*.db` is
  gitignored because it can contain the saved key.
