#!/usr/bin/env bash
# Export the two project-specific ONNX models (speaker embedding + age/gender)
# and upload them to the HF repo media-ai downloads by default. Run once.
#
#   huggingface-cli login   # or export HF_TOKEN=...
#   make upload-models      # or: bash tools/upload-models.sh
#
# The pyannote segmentation model is NOT uploaded here — media-ai defaults to
# sherpa-onnx's public, non-gated copy.
set -euo pipefail

REPO="${MODELS_REPO:-trinvhco/ruin-media-ai}"
OUT="${OUT:-/tmp/ruin-media-ai-models}"
PY=(uv run --with torch --with "transformers==4.40.2" --with onnx --with onnxscript --with "numpy<2" python)
HF=(uv run --with "huggingface_hub[cli]" huggingface-cli)

mkdir -p "$OUT"

echo "==> exporting speaker-embedding (WavLM-base-plus-sv) → speaker-embedding.onnx"
"${PY[@]}" tools/export-speaker-embedding-onnx.py --out "$OUT/speaker-embedding.onnx"

echo "==> exporting age/gender (audeering wav2vec2) → agegender.onnx"
"${PY[@]}" tools/export-agegender-onnx.py --out "$OUT/agegender.onnx"

echo "==> uploading to https://huggingface.co/$REPO"
"${HF[@]}" upload "$REPO" "$OUT/speaker-embedding.onnx" speaker-embedding.onnx
"${HF[@]}" upload "$REPO" "$OUT/agegender.onnx" agegender.onnx

echo "done — media-ai will download these by default (MEDIA_AI_EMBED_REPO/_AGEGENDER_REPO=$REPO)."
