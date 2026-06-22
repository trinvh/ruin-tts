#!/usr/bin/env bash
# Full hot-reload dev loop for Beesoft Studio.
#
# Runs the three Rust sidecars under `cargo watch` on FIXED localhost ports, then
# launches the Tauri app pointed at them (DEV_*_BASE → the shell connects instead
# of spawning). Editing any Rust crate rebuilds + restarts just that sidecar; the
# running app picks up the new code on its next request — no manual build, no app
# restart. The UI hot-reloads via Vite as usual.
#
# Profiles: studio = debug (fast rebuilds, no ML at runtime); vieneu + media-ai =
# release (ML inference needs the optimised build; they change rarely).
set -euo pipefail
cd "$(dirname "$0")/.."

TTS_PORT=8080
STUDIO_PORT=8090
MEDIA_PORT=8099
# Absolute so artifact paths (export.mp4, audio.wav…) the studio stores resolve
# from any process — the Tauri shell that copies the exported file runs from a
# different working directory than this script's studio.
DEV_DIR="$(pwd)/.dev"
mkdir -p "$DEV_DIR/studio-work"

# cargo-watch is required; offer to install it once.
if ! cargo watch --version >/dev/null 2>&1; then
  echo "→ cargo-watch not found. Installing (one-time)…"
  cargo install cargo-watch
fi

# media-ai needs HF_TOKEN for some model downloads (mirrors how the user runs it).
if [ -f services/media-ai/.env ]; then
  set -a; . services/media-ai/.env; set +a
fi

# Prefer the full static ffmpeg the app downloads during onboarding (it has
# libfreetype/libass → drawtext + subtitle burn). The packaged shell passes this
# to its sidecars, but in dev studio runs from this script, so wire it here.
APP_BIN="$HOME/Library/Application Support/com.trinvh.beesoft/bin"
if [ -x "$APP_BIN/ffmpeg" ]; then
  export FFMPEG_PATH="$APP_BIN/ffmpeg"
  [ -x "$APP_BIN/ffprobe" ] && export FFPROBE_PATH="$APP_BIN/ffprobe"
  echo "→ using app-downloaded ffmpeg ($APP_BIN/ffmpeg)"
fi

# Kill any sidecars left from a previous run on the dev ports.
pkill -f "vieneu-server --addr 127.0.0.1:${TTS_PORT}"  2>/dev/null || true
pkill -f "studio-server --addr 127.0.0.1:${STUDIO_PORT}" 2>/dev/null || true
pkill -f "media-ai --addr 127.0.0.1:${MEDIA_PORT}"    2>/dev/null || true

PIDS=()
cleanup() {
  echo; echo "→ shutting down dev sidecars…"
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  pkill -f "vieneu-server --addr 127.0.0.1:${TTS_PORT}"  2>/dev/null || true
  pkill -f "studio-server --addr 127.0.0.1:${STUDIO_PORT}" 2>/dev/null || true
  pkill -f "media-ai --addr 127.0.0.1:${MEDIA_PORT}"    2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "→ starting watched sidecars (first build may take a minute)…"

# vieneu-server (TTS) — release, watch the engine + server crates.
cargo watch -q -w crates/vieneu-core -w crates/vieneu-server \
  -x "run --release -p vieneu-server -- --addr 127.0.0.1:${TTS_PORT} --workers 2" &
PIDS+=($!)

# media-ai (ASR + diarization) — release.
cargo watch -q -w crates/media-ai \
  -x "run --release -p media-ai -- --addr 127.0.0.1:${MEDIA_PORT}" &
PIDS+=($!)

# studio-server — debug (fast rebuilds); reaches the others via these bases.
VIENEU_BASE="http://127.0.0.1:${TTS_PORT}" \
MEDIA_AI_BASE="http://127.0.0.1:${MEDIA_PORT}" \
cargo watch -q -w crates/studio \
  -x "run -p studio --bin studio-server -- --addr 127.0.0.1:${STUDIO_PORT} --db ${DEV_DIR}/studio.db --work-dir ${DEV_DIR}/studio-work" &
PIDS+=($!)

# Tell the Tauri shell to connect to the watched sidecars instead of spawning.
export DEV_VIENEU_BASE="http://127.0.0.1:${TTS_PORT}"
export DEV_STUDIO_BASE="http://127.0.0.1:${STUDIO_PORT}"
export DEV_MEDIA_AI_BASE="http://127.0.0.1:${MEDIA_PORT}"

echo "→ launching Tauri app (UI hot-reload via Vite)…"
pnpm -C ui install
pnpm -C ui tauri dev
