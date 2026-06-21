"""Speaker diarization via pyannote (community-1 model).

Returns a list of (start, end, speaker_label) turns. The pipeline is loaded once
and cached; it needs a HuggingFace token (HF_TOKEN) with the model conditions
accepted.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from functools import lru_cache


@dataclass
class Turn:
    start: float
    end: float
    speaker: str


_MODEL = os.environ.get("MEDIA_AI_DIARIZE_MODEL", "pyannote/speaker-diarization-community-1")


@lru_cache(maxsize=1)
def _pipeline():
    import torch
    from pyannote.audio import Pipeline

    token = os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_TOKEN")
    if not token:
        raise RuntimeError(
            "HF_TOKEN chưa được cấu hình — cần token HuggingFace đã chấp nhận điều khoản "
            f"của model {_MODEL}. Đặt trong services/media-ai/.env"
        )
    pipe = Pipeline.from_pretrained(_MODEL, token=token)
    if pipe is None:
        raise RuntimeError(
            f"Không tải được pipeline {_MODEL}. Kiểm tra token và đã accept điều khoản model chưa."
        )
    # Prefer Apple GPU when available; pyannote falls back to CPU if a layer is
    # unsupported on MPS.
    if torch.backends.mps.is_available():
        try:
            pipe.to(torch.device("mps"))
        except Exception:
            pass
    elif torch.cuda.is_available():
        pipe.to(torch.device("cuda"))
    return pipe


def _to_annotation(out):
    """pyannote 3.x returns an Annotation directly; community-1 (4.x) wraps it in
    a DiarizeOutput. Unwrap to the object exposing `.itertracks`."""
    if hasattr(out, "itertracks"):
        return out
    for attr in (
        "speaker_diarization",
        "exclusive_speaker_diarization",
        "diarization",
        "prediction",
    ):
        ann = getattr(out, attr, None)
        if ann is not None and hasattr(ann, "itertracks"):
            return ann
    raise RuntimeError(
        f"Không hiểu kết quả diarization: {type(out).__name__} "
        f"(thuộc tính: {[a for a in dir(out) if not a.startswith('_')]})"
    )


def diarize(audio_path: str, num_speakers: int | None = None) -> list[Turn]:
    pipe = _pipeline()
    kwargs = {}
    if num_speakers:
        kwargs["num_speakers"] = num_speakers
    annotation = _to_annotation(pipe(audio_path, **kwargs))
    turns: list[Turn] = []
    for segment, _, label in annotation.itertracks(yield_label=True):
        turns.append(Turn(start=float(segment.start), end=float(segment.end), speaker=str(label)))
    turns.sort(key=lambda t: t.start)
    return turns
