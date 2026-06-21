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
from transformers import Wav2Vec2Model, Wav2Vec2PreTrainedModel

MODEL_ID = "audeering/wav2vec2-large-robust-24-ft-age-gender"


# The audeering model isn't a stock AutoModel — its age + gender heads must be
# declared so the checkpoint's `age.*` / `gender.*` weights load (otherwise they
# are silently dropped). Class copied from the model card.
class ModelHead(nn.Module):
    def __init__(self, config, num_labels):
        super().__init__()
        self.dense = nn.Linear(config.hidden_size, config.hidden_size)
        self.dropout = nn.Dropout(config.final_dropout)
        self.out_proj = nn.Linear(config.hidden_size, num_labels)

    def forward(self, x):
        x = self.dropout(x)
        x = self.dense(x)
        x = torch.tanh(x)
        x = self.dropout(x)
        return self.out_proj(x)


class AgeGenderModel(Wav2Vec2PreTrainedModel):
    def __init__(self, config):
        super().__init__(config)
        self.config = config
        self.wav2vec2 = Wav2Vec2Model(config)
        self.age = ModelHead(config, 1)
        self.gender = ModelHead(config, 3)
        self.init_weights()

    def forward(self, input_values):
        hidden = self.wav2vec2(input_values)[0]
        pooled = torch.mean(hidden, dim=1)
        logits_age = self.age(pooled)
        logits_gender = torch.softmax(self.gender(pooled), dim=1)
        return logits_age, logits_gender


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="model.onnx")
    ap.add_argument("--opset", type=int, default=17)
    args = ap.parse_args()

    model = AgeGenderModel.from_pretrained(MODEL_ID)
    model.eval()
    dummy = torch.zeros(1, 16_000, dtype=torch.float32)  # 1 s @ 16 kHz

    torch.onnx.export(
        model,
        (dummy,),
        args.out,
        input_names=["input_values"],
        output_names=["logits_age", "logits_gender"],
        dynamic_axes={"input_values": {1: "n_samples"}},
        opset_version=args.opset,
        do_constant_folding=True,
        dynamo=False,
    )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
