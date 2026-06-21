#!/usr/bin/env python3
"""Export audeering's wav2vec2 age/gender model to ONNX for the Rust media-ai
sidecar (`crates/media-ai/src/agegender.rs`).

The model returns hidden states + age (regression, 0..1) + gender logits
(3 classes: female, male, child). We wrap it so the ONNX graph exposes exactly
two named outputs the Rust side reads: `logits_age` [1,1] and `logits_gender`
[1,3], with input `input_values` [1, n_samples] (raw 16 kHz mono waveform; the
Rust side z-normalizes before calling).

Usage:
    pip install torch transformers onnx
    python tools/export-agegender-onnx.py --out model.onnx
    # then host model.onnx (e.g. a HF repo) and run the sidecar with
    #   MEDIA_AI_AGEGENDER_PATH=/path/to/model.onnx   (or _REPO/_MODEL)

Reference: https://huggingface.co/audeering/wav2vec2-large-robust-24-ft-age-gender
"""

from __future__ import annotations

import argparse

import torch
import torch.nn as nn
from transformers import AutoModel

MODEL_ID = "audeering/wav2vec2-large-robust-24-ft-age-gender"


class AgeGenderOnnx(nn.Module):
    """Thin wrapper exposing (logits_age, logits_gender) from the audeering model."""

    def __init__(self, model_id: str = MODEL_ID):
        super().__init__()
        # trust_remote_code: the repo ships a custom head returning age + gender.
        self.model = AutoModel.from_pretrained(model_id, trust_remote_code=True)
        self.model.eval()

    def forward(self, input_values: torch.Tensor):
        out = self.model(input_values)
        # The custom model returns (hidden_states, logits_age, logits_gender)
        # as a tuple; fall back to attribute access if it's an object.
        if isinstance(out, (tuple, list)):
            _, logits_age, logits_gender = out
        else:
            logits_age = out.logits_age
            logits_gender = out.logits_gender
        return logits_age, logits_gender


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="model.onnx")
    ap.add_argument("--opset", type=int, default=17)
    args = ap.parse_args()

    model = AgeGenderOnnx()
    # 1 second of dummy audio at 16 kHz.
    dummy = torch.zeros(1, 16_000, dtype=torch.float32)

    torch.onnx.export(
        model,
        (dummy,),
        args.out,
        input_names=["input_values"],
        output_names=["logits_age", "logits_gender"],
        dynamic_axes={
            "input_values": {1: "n_samples"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
    )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
