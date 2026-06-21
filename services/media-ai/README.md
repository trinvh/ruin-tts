# media-ai — audio analysis sidecar

Heavy ML for ruin-studio's video dubbing: ASR (mlx-whisper on Apple Silicon),
speaker diarization (pyannote community-1), and per-speaker age/gender
(audeering wav2vec2). Stateless and file-path based — studio extracts a 16 kHz
mono wav into its work dir and posts the path here.

## Setup (one-time)

```bash
cd services/media-ai
cp .env.example .env          # then paste your HF_TOKEN
# accept terms: https://huggingface.co/pyannote/speaker-diarization-community-1

# with uv (recommended):
uv venv && uv pip install -e .
# or with pip:
python3 -m venv .venv && . .venv/bin/activate && pip install -e .
```

## Run

```bash
# from services/media-ai (the user runs this themselves, like vieneu-server)
uv run uvicorn app:app --host 127.0.0.1 --port 8099
# or: . .venv/bin/activate && uvicorn app:app --host 127.0.0.1 --port 8099
```

Studio talks to it at `media_ai_base` (default `http://127.0.0.1:8099`, set in
the app's Settings page).

## API

`GET  /health` → `{ "status": "ok", "hf_token": true }`

`POST /analyze` → body `{ "audio_path": "/abs/path.wav", "hint_lang": "zh"?, "num_speakers": 2? }`

```json
{
  "language": "zh",
  "segments": [
    { "id": 0, "start": 0.0, "end": 3.2, "speaker": "SPEAKER_00", "text_src": "...", "lang": "zh" }
  ],
  "speakers": [
    { "speaker": "SPEAKER_00", "gender": "female", "age": 31.4 }
  ]
}
```

## Notes

- First run downloads the models (a few GB). Keep them warm by leaving the
  server running.
- `gender` can be `null` if the age-gender model can't load — the flow still
  works; map voices by hand in the UI.
- On Apple Silicon, pyannote uses MPS when possible and falls back to CPU per
  layer; a few-minute clip is fine.
