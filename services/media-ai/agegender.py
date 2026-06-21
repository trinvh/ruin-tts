"""Per-speaker age + gender estimation.

Uses audeering's wav2vec2 age-gender model. This is best-effort: if the model
fails to load (offline, incompatible transformers), callers get gender=None and
the dubbing flow continues — the operator can still map voices by hand.
"""

from __future__ import annotations

import os
import traceback
from dataclasses import dataclass
from functools import lru_cache

import numpy as np

_MODEL = os.environ.get(
    "MEDIA_AI_AGEGENDER_MODEL", "audeering/wav2vec2-large-robust-24-ft-age-gender"
)
# audeering model gender head order: female, male, child.
_GENDERS = ["female", "male", "child"]

# Last load/predict error, surfaced so the operator can see WHY gender is missing.
_LAST_ERROR: str | None = None


@dataclass
class AgeGender:
    gender: str | None
    age: float | None  # years


def last_error() -> str | None:
    return _LAST_ERROR


def _load_state_dict(repo: str) -> dict:
    """Download the model weights directly (safetensors first, then .bin)."""
    import torch
    from huggingface_hub import hf_hub_download

    try:
        import safetensors.torch as st

        path = hf_hub_download(repo, "model.safetensors")
        return st.load_file(path)
    except Exception:
        path = hf_hub_download(repo, "pytorch_model.bin")
        return torch.load(path, map_location="cpu", weights_only=True)


def _build_model():
    import torch
    import torch.nn as nn
    from transformers import Wav2Vec2Config, Wav2Vec2FeatureExtractor
    from transformers.models.wav2vec2.modeling_wav2vec2 import (
        Wav2Vec2Model,
        Wav2Vec2PreTrainedModel,
    )

    class ModelHead(nn.Module):
        def __init__(self, config, num_labels):
            super().__init__()
            self.dense = nn.Linear(config.hidden_size, config.hidden_size)
            self.dropout = nn.Dropout(config.final_dropout)
            self.out_proj = nn.Linear(config.hidden_size, num_labels)

        def forward(self, x):
            x = self.dropout(x)
            x = torch.tanh(self.dense(x))
            x = self.dropout(x)
            return self.out_proj(x)

    class AgeGenderModel(Wav2Vec2PreTrainedModel):
        def __init__(self, config):
            super().__init__(config)
            self.wav2vec2 = Wav2Vec2Model(config)
            self.age = ModelHead(config, 1)
            self.gender = ModelHead(config, len(_GENDERS))

        def forward(self, input_values):
            hidden = self.wav2vec2(input_values)[0]
            pooled = torch.mean(hidden, dim=1)
            age = self.age(pooled)
            gender = torch.softmax(self.gender(pooled), dim=1)
            return age, gender

    # Build + load weights MANUALLY (not via from_pretrained): newer transformers
    # (>=4.50) changed the loading path and chokes on this old-style custom model
    # ("no attribute 'all_tied_weights_keys'"). Manual load sidesteps that and is
    # version-robust. The model is feature-extractor only (no tokenizer).
    fe = Wav2Vec2FeatureExtractor.from_pretrained(_MODEL)
    config = Wav2Vec2Config.from_pretrained(_MODEL)
    model = AgeGenderModel(config)
    missing, unexpected = model.load_state_dict(_load_state_dict(_MODEL), strict=False)
    # The classifier heads must be present; the wav2vec2 backbone too.
    if any(k.startswith(("age.", "gender.")) for k in missing):
        raise RuntimeError(f"thiếu trọng số head age/gender: {missing}")
    model.eval()
    return fe, model


@lru_cache(maxsize=1)
def _model_or_none():
    global _LAST_ERROR
    try:
        m = _build_model()
        _LAST_ERROR = None
        return m
    except Exception as e:  # noqa: BLE001 - degrade gracefully but report
        _LAST_ERROR = f"{type(e).__name__}: {e}"
        print("[media-ai] age-gender model unavailable:\n" + traceback.format_exc(), flush=True)
        return None


def predict(samples: np.ndarray, sr: int = 16000) -> AgeGender:
    """Predict on a mono 16 kHz waveform slice for one speaker."""
    global _LAST_ERROR
    built = _model_or_none()
    if built is None or samples.size == 0:
        return AgeGender(gender=None, age=None)
    try:
        import torch

        fe, model = built
        inputs = fe(samples, sampling_rate=sr, return_tensors="pt")
        with torch.no_grad():
            age_logits, gender_probs = model(inputs["input_values"])
        gi = int(torch.argmax(gender_probs, dim=1).item())
        return AgeGender(gender=_GENDERS[gi], age=round(float(age_logits.item()) * 100.0, 1))
    except Exception as e:  # noqa: BLE001
        _LAST_ERROR = f"{type(e).__name__}: {e}"
        print("[media-ai] age-gender predict failed:\n" + traceback.format_exc(), flush=True)
        return AgeGender(gender=None, age=None)
