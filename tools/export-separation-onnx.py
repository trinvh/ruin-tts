#!/usr/bin/env python3
"""Export a 2-speaker ConvTasNet (asteroid, Libri2Mix 16 kHz) to ONNX for the
Rust media-ai overlap separation (`crates/media-ai/src/separate.rs`).

Input  `mix`     [1, n]        (raw 16 kHz mono mixed waveform)
Output `sources` [1, 2, n]     (separated per-speaker streams)

Usage:
    pip install asteroid requests onnx onnxscript "numpy<2"
    python tools/export-separation-onnx.py --out separation.onnx
"""

from __future__ import annotations

import argparse

import torch
from asteroid.models import ConvTasNet

MODEL_ID = "JorisCos/ConvTasNet_Libri2Mix_sepclean_16k"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="separation.onnx")
    ap.add_argument("--opset", type=int, default=16)
    args = ap.parse_args()

    model = ConvTasNet.from_pretrained(MODEL_ID)
    model.eval()
    dummy = torch.randn(1, 16_000)

    torch.onnx.export(
        model,
        (dummy,),
        args.out,
        input_names=["mix"],
        output_names=["sources"],
        dynamic_axes={"mix": {1: "n"}, "sources": {2: "n"}},
        opset_version=args.opset,
        do_constant_folding=True,
        dynamo=False,
    )
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
