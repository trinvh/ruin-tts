"""Speech recognition with word/segment timestamps.

On Apple Silicon this uses mlx-whisper (GPU via MLX). Everywhere else — including
Intel Macs (MLX is Apple-Silicon-only), Windows and Linux — it falls back to
openai-whisper (CPU). Both return the same normalized segment shape.
"""

from __future__ import annotations

import os
import platform
import sys
from dataclasses import dataclass


@dataclass
class AsrSegment:
    start: float
    end: float
    text: str


@dataclass
class AsrResult:
    language: str
    segments: list[AsrSegment]


# MLX only ships for Apple Silicon — Intel Macs must use the CPU path.
_IS_APPLE_SILICON = sys.platform == "darwin" and platform.machine() == "arm64"
# large-v3-turbo: fast + accurate enough for dubbing source transcription.
_MLX_MODEL = os.environ.get("MEDIA_AI_WHISPER_MODEL", "mlx-community/whisper-large-v3-turbo")
_CPU_MODEL = os.environ.get("MEDIA_AI_WHISPER_MODEL_CPU", "large-v3")


def transcribe(audio_path: str, hint_lang: str | None = None) -> AsrResult:
    """Transcribe `audio_path`, auto-detecting language unless `hint_lang` given."""
    if _IS_APPLE_SILICON:
        return _transcribe_mlx(audio_path, hint_lang)
    return _transcribe_cpu(audio_path, hint_lang)


def _transcribe_mlx(audio_path: str, hint_lang: str | None) -> AsrResult:
    import mlx_whisper

    out = mlx_whisper.transcribe(
        audio_path,
        path_or_hf_repo=_MLX_MODEL,
        language=hint_lang,
        word_timestamps=True,
        condition_on_previous_text=False,
    )
    return _normalize(out)


def _transcribe_cpu(audio_path: str, hint_lang: str | None) -> AsrResult:
    import whisper

    model = whisper.load_model(_CPU_MODEL)
    out = model.transcribe(
        audio_path,
        language=hint_lang,
        word_timestamps=True,
        condition_on_previous_text=False,
    )
    return _normalize(out)


def _normalize(out: dict) -> AsrResult:
    lang = out.get("language") or "unknown"
    segments: list[AsrSegment] = []
    for s in out.get("segments", []):
        text = (s.get("text") or "").strip()
        if not text:
            continue
        segments.append(
            AsrSegment(
                start=float(s.get("start", 0.0)),
                end=float(s.get("end", 0.0)),
                text=text,
            )
        )
    return AsrResult(language=lang, segments=segments)
