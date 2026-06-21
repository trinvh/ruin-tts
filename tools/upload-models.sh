#!/usr/bin/env bash
# Export the project-specific ONNX models (speaker embedding, age/gender,
# separation) and upload them to the HF repo media-ai downloads by default.
# Run once.
#
#   hf auth login           # or export HF_TOKEN=...
#   make upload-models      # or: bash tools/upload-models.sh
#
# Already-exported files in $OUT are reused, so re-running (e.g. after an upload
# error) is instant. The pyannote segmentation model is NOT uploaded here —
# media-ai defaults to sherpa-onnx's public, non-gated copy.
set -euo pipefail

REPO="${MODELS_REPO:-trinvh/ruin-media-ai}"
OUT="${OUT:-/tmp/ruin-media-ai-models}"
PY=(uv run --with torch --with "transformers==4.40.2" --with onnx --with onnxscript --with "numpy<2" python)
SEP=(uv run --with asteroid --with requests --with onnx --with onnxscript --with "numpy<2" python)
# The modern CLI is `hf` (huggingface-cli is deprecated). Token via `hf auth
# login` or HF_TOKEN; the repo is auto-created on first upload.
HF=(uv run --with "huggingface_hub" hf)

mkdir -p "$OUT"

export_if_missing() { # <out-file> <python-cmd-array-name> <script>
  local out="$1"; shift
  if [[ -s "$out" ]]; then
    echo "==> $(basename "$out") already exported — skipping"
  else
    "$@" --out "$out"
  fi
}

echo "==> exporting models (reusing any already in $OUT)…"
export_if_missing "$OUT/speaker-embedding.onnx" "${PY[@]}" tools/export-speaker-embedding-onnx.py
export_if_missing "$OUT/agegender.onnx" "${PY[@]}" tools/export-agegender-onnx.py
export_if_missing "$OUT/separation.onnx" "${SEP[@]}" tools/export-separation-onnx.py

echo "==> uploading to https://huggingface.co/$REPO"
"${HF[@]}" upload "$REPO" "$OUT/speaker-embedding.onnx" speaker-embedding.onnx
"${HF[@]}" upload "$REPO" "$OUT/agegender.onnx" agegender.onnx
"${HF[@]}" upload "$REPO" "$OUT/separation.onnx" separation.onnx

echo "done — media-ai downloads these by default (MEDIA_AI_EMBED_REPO/_AGEGENDER_REPO/_SEPARATE_REPO=$REPO)."
