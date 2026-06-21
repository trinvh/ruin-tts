#!/usr/bin/env python3
"""Export Microsoft WavLM-base-plus-sv (speaker verification) to ONNX for the
Rust media-ai diarization (`crates/media-ai`). It's a proper speaker-embedding
model (trained on VoxCeleb) — far more discriminative than the age/gender hidden
state, especially for same-gender / many speakers.

Input  `input_values` [1, n_samples]  (raw 16 kHz mono waveform; Rust z-normalizes)
Output `embedding`     [1, 512]        (L2-normalized x-vector, for cosine clustering)

Usage:
    pip install torch transformers onnx onnxscript "numpy<2"
    python tools/export-speaker-embedding-onnx.py --out speaker.onnx
    # then host it and run media-ai with MEDIA_AI_EMBED_PATH / _REPO
"""

from __future__ import annotations

import argparse

import torch
import torch.nn as nn
from transformers import WavLMForXVector

MODEL_ID = "microsoft/wavlm-base-plus-sv"


class SpeakerEmbedding(nn.Module):
    def __init__(self, model_id: str = MODEL_ID):
        super().__init__()
        self.model = WavLMForXVector.from_pretrained(model_id)
        self.model.eval()

    def forward(self, input_values):
        emb = self.model(input_values).embeddings  # [1, 512]
        return torch.nn.functional.normalize(emb, dim=1)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="speaker.onnx")
    ap.add_argument("--opset", type=int, default=17)
    args = ap.parse_args()

    model = SpeakerEmbedding()
    dummy = torch.zeros(1, 16_000, dtype=torch.float32)  # 1 s @ 16 kHz

    torch.onnx.export(
        model,
        (dummy,),
        args.out,
        input_names=["input_values"],
        output_names=["embedding"],
        dynamic_axes={"input_values": {1: "n_samples"}},
        opset_version=args.opset,
        do_constant_folding=True,
        dynamo=False,
    )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
