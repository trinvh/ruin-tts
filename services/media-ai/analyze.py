"""Combine ASR + diarization + age/gender into dubbing-ready segments."""

from __future__ import annotations

import numpy as np

import agegender
import asr
import diarize


def _speaker_for(start: float, end: float, turns: list[diarize.Turn]) -> str:
    """Diarization speaker with the greatest temporal overlap with [start, end]."""
    best, best_ov = None, 0.0
    for t in turns:
        ov = max(0.0, min(end, t.end) - max(start, t.start))
        if ov > best_ov:
            best_ov, best = ov, t.speaker
    return best or "SPEAKER_00"


def _speaker_samples(
    audio: np.ndarray, sr: int, segs: list[dict], speaker: str, max_seconds: float = 12.0
) -> np.ndarray:
    """Concatenate up to `max_seconds` of the longest segments for one speaker."""
    own = sorted(
        (s for s in segs if s["speaker"] == speaker),
        key=lambda s: s["end"] - s["start"],
        reverse=True,
    )
    chunks: list[np.ndarray] = []
    total = 0.0
    for s in own:
        a, b = int(s["start"] * sr), int(s["end"] * sr)
        chunks.append(audio[a:b])
        total += s["end"] - s["start"]
        if total >= max_seconds:
            break
    return np.concatenate(chunks) if chunks else np.zeros(0, dtype=np.float32)


def analyze(audio_path: str, hint_lang: str | None = None, num_speakers: int | None = None) -> dict:
    import librosa

    result = asr.transcribe(audio_path, hint_lang)
    turns = diarize.diarize(audio_path, num_speakers)

    segments: list[dict] = []
    for i, seg in enumerate(result.segments):
        segments.append(
            {
                "id": i,
                "start": round(seg.start, 3),
                "end": round(seg.end, 3),
                "speaker": _speaker_for(seg.start, seg.end, turns),
                "text_src": seg.text,
                "lang": result.language,
            }
        )

    # Per-speaker age/gender on a representative concatenation.
    audio, sr = librosa.load(audio_path, sr=16000, mono=True)
    speakers: list[dict] = []
    for spk in sorted({s["speaker"] for s in segments}):
        samples = _speaker_samples(audio, sr, segments, spk)
        ag = agegender.predict(samples, sr)
        speakers.append({"speaker": spk, "gender": ag.gender, "age": ag.age})

    out = {"language": result.language, "segments": segments, "speakers": speakers}
    note = agegender.last_error()
    if note and all(s["gender"] is None for s in speakers):
        out["gender_note"] = note
    return out
