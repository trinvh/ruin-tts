"""media-ai sidecar: audio analysis for ruin-studio video dubbing.

Stateless, file-path based — studio extracts a 16 kHz mono wav into its work dir
and passes the path here. One synchronous endpoint (clips are short).

Run:  uv run uvicorn app:app --host 127.0.0.1 --port 8099
"""

from __future__ import annotations

import os

from dotenv import load_dotenv
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

load_dotenv()

import analyze as analyze_mod  # noqa: E402  (after load_dotenv so env is set)

app = FastAPI(title="media-ai", version="0.1.0")


class AnalyzeRequest(BaseModel):
    audio_path: str
    hint_lang: str | None = None
    num_speakers: int | None = None


@app.get("/health")
def health() -> dict:
    return {"status": "ok", "hf_token": bool(os.environ.get("HF_TOKEN"))}


@app.post("/analyze")
def analyze(req: AnalyzeRequest) -> dict:
    if not os.path.isfile(req.audio_path):
        raise HTTPException(status_code=400, detail=f"audio not found: {req.audio_path}")
    try:
        return analyze_mod.analyze(req.audio_path, req.hint_lang, req.num_speakers)
    except Exception as e:  # noqa: BLE001
        raise HTTPException(status_code=500, detail=str(e)) from e
