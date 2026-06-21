# Validating the Rust media-ai sidecar

The Rust sidecar (`crates/media-ai`) serves the same `POST /analyze` + `/health`
contract as the Python one. Validate it in layers — fastest/cheapest first.

Status of each component (so you know what to expect):
- **ASR** (whisper.cpp) — implemented; validate transcript + timestamps.
- **Age/gender** (audeering wav2vec2 → ONNX via `ort`) — implemented; needs the
  exported model; validate gender/age are populated and plausible.
- **Diarization** — single-speaker fallback only; every segment → `SPEAKER_00`.
  (Multi-speaker is still on the Python sidecar.)

---

## 0. Unit tests (already green, no models needed)

```bash
cargo test -p media-ai     # 11 passing: decode math, orchestration, downmix
```
Validates the pure logic (age/gender decode, speaker assignment/sampling, audio
downmix). Inference is validated below.

## 1. Build + run

```bash
# macOS needs cmake (whisper.cpp); Intel Mac also needs ORT_DYLIB_PATH (docs/MACOS-INTEL.md)
cargo run -p media-ai --release -- --addr 127.0.0.1:8099
# first run downloads ggml-large-v3-turbo (~1.5 GB) from HF
curl -s http://127.0.0.1:8099/health        # → {"status":"ok","impl":"rust"}
```

## 2. ASR — transcript + timestamps

Make a 16 kHz mono WAV (what studio feeds it) and call `/analyze`:

```bash
ffmpeg -y -i your_video.mp4 -vn -ac 1 -ar 16000 /tmp/clip16k.wav
curl -s -X POST http://127.0.0.1:8099/analyze \
  -H 'content-type: application/json' \
  -d '{"audio_path":"/tmp/clip16k.wav"}' | jq
```

**Correct looks like:**
- `language` is right (e.g. `"zh"` / `"en"`).
- `segments[]` non-empty; each has readable `text_src`, and `start < end` with
  times increasing and within the clip's duration.
- Spot-check: seek your video to a segment's `start` — the speech should match
  the text.

## 3. ASR parity vs the Python sidecar (ground truth)

Run both on different ports and diff the same clip. Both use large-v3-turbo, so
ASR should be near-identical (diarization will differ — Rust is single-speaker).

```bash
# terminal A — Python (full-featured)
make media-ai-run                                   # :8099
# terminal B — Rust
cargo run -p media-ai --release -- --addr 127.0.0.1:8098

REQ='{"audio_path":"/tmp/clip16k.wav"}'
curl -s -XPOST localhost:8099/analyze -H 'content-type: application/json' -d "$REQ" \
  | jq '{language, n:(.segments|length), text:[.segments[].text_src]}' > /tmp/py.json
curl -s -XPOST localhost:8098/analyze -H 'content-type: application/json' -d "$REQ" \
  | jq '{language, n:(.segments|length), text:[.segments[].text_src]}' > /tmp/rs.json
diff /tmp/py.json /tmp/rs.json && echo "ASR matches"
```

## 4. Age/gender — export the model, then check output

```bash
# export once (any machine with torch)
pip install torch transformers onnx
python tools/export-agegender-onnx.py --out /tmp/agegender.onnx

# run the sidecar pointed at it
MEDIA_AI_AGEGENDER_PATH=/tmp/agegender.onnx \
  cargo run -p media-ai --release -- --addr 127.0.0.1:8099

curl -s -XPOST localhost:8099/analyze -H 'content-type: application/json' \
  -d '{"audio_path":"/tmp/clip16k.wav"}' | jq '.speakers'
```

**Correct looks like:** `speakers[].gender` is `"male"`/`"female"`/`"child"`
(not null) and `age` is a plausible number of years. Compare to the Python
sidecar's `speakers` on the same clip — they use the same model, so values
should be close.

If `gender`/`age` stay null and the log shows an `age/gender inference lỗi`, the
ONNX output names differ from `logits_age` / `logits_gender` — check the export's
output names and adjust `tools/export-agegender-onnx.py` (or `agegender.rs`).

Sanity-check the export itself (independent of Rust):

```python
import onnxruntime as ort, numpy as np
s = ort.InferenceSession("/tmp/agegender.onnx")
x = np.zeros((1, 16000), np.float32)
print([o.name for o in s.get_outputs()])          # expect logits_age, logits_gender
print(s.run(None, {"input_values": x}))
```

## 5. End-to-end in studio

`studio` calls whatever is on `media_ai_base` (default `http://127.0.0.1:8099`,
editable in the app Settings). So just run the Rust sidecar on :8099, then:

1. Open a dub project, run **Tách tiếng** (extract) — produces the 16 kHz wav.
2. Run **Phân tích** (analyze) — hits the Rust sidecar.
3. Confirm the transcript/segments appear and read correctly. Multi-speaker
   videos will show one speaker until diarization is ported.

Switch back to the Python sidecar anytime by running it on :8099 instead (or
point `media_ai_base` at a different port).
