# Clip-based timeline (NLE) — design & plan

Status: **proposed** (awaiting approval before Phase 0). Owner: dub/Video Studio.

## 1. Goal

Make the Video Studio timeline the **source of truth**: a multi-track set of
**clips** (video / audio / image / text) the user can **add, freely move, trim,
restack**, with **preview and export faithfully reflecting** clip positions. Every
clip's geometry/timing is stored **per clip** (never per-project). The dubbing
pipeline (ASR → translate → TTS) becomes **one way to populate clips**, not the
data model itself.

This replaces the dub-specific timeline (1 source video + segments + overlays +
the interim `video_offset_s`) with a general compositor.

## 2. Why (the gap today)

- "Add media" buttons (`useStudio.addVideo/addImage/addMusic`) make **visual-only
  placeholders** — not persisted, not in preview, not in export.
- Real added media today = only **banner overlays** (image, burned into export).
- Positions live in mixed places: segment `offset_s` (per-segment ✓), overlay
  geometry (per-overlay ✓), but the **source video position is per-project**
  (`video_offset_s`) — the wrong shape, flagged by the user.
- Export is dub-specific (1 video + 1 VN track + subs/overlays), not a compositor.

## 3. Data model — `dub_clips`

One row per clip; the timeline is rebuilt from these, and export composites them.

```sql
CREATE TABLE dub_clips (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  track       INTEGER NOT NULL,   -- lane index; also the compositing z-order
  kind        TEXT NOT NULL,      -- 'video' | 'audio' | 'image' | 'text'
  source      TEXT,               -- file path (video/audio/image); NULL for text
  start_s     REAL NOT NULL,      -- placement on the timeline
  dur_s       REAL NOT NULL,      -- length on the timeline
  in_s        REAL NOT NULL DEFAULT 0,   -- trim in-point inside the source media
  volume      REAL NOT NULL DEFAULT 1,   -- audio gain (video/audio)
  x           REAL NOT NULL DEFAULT 0,   -- fractional pos/size for visual clips
  y           REAL NOT NULL DEFAULT 0,
  w           REAL NOT NULL DEFAULT 1,
  opacity     REAL NOT NULL DEFAULT 1,
  text        TEXT,               -- text/subtitle content
  text_style  TEXT,               -- JSON: size/color/bg/align for text clips
  origin      TEXT,               -- provenance: 'user' | 'dub:video' | 'dub:tts:<segId>' | 'dub:sub:<segId>' | 'dub:banner'
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_dub_clips_project ON dub_clips(project_id);
```

`origin` lets the dub pipeline regenerate its clips (delete `origin LIKE 'dub:%'`
and rewrite) without touching user-added clips.

### Mapping existing entities → clips
- Source video → `kind=video, origin='dub:video'`, `start_s` = lead-in (replaces
  `video_offset_s`).
- Original audio → carried by the video clip's `volume` (no separate clip), or a
  derived audio clip if we want it independently movable later.
- Each TTS dub line → `kind=audio, origin='dub:tts:<segId>', source=fitted_path,
  start_s=placed`. (Replaces per-segment `offset_s` with the clip's `start_s`.)
- Subtitles → `kind=text, origin='dub:sub:<segId>'` (or keep deriving from
  segments in Phase 0; convert in Phase 2).
- Banners → `kind=image, origin='dub:banner'` (the existing overlays).

## 4. Backend

- **CRUD**: `/api/dub/projects/{id}/clips` (list/create), `/api/dub/clips/{cid}`
  (update geometry/timing, delete), `/api/dub/clips/{cid}/media` (serve file).
  Upload via multipart (reuse the overlay upload pattern).
- **Compose step**: a pipeline step generates `dub:*` clips from the current dub
  state (video + tts + subs + banners) so the timeline + export read one model.
  Re-run after synth/build, idempotent by `origin`.
- **Generalized export** (`media.rs`): build an ffmpeg graph from the clips —
  - video layers: `scale` to `w`, position via `overlay=W*x:H*y`, time-gate via
    `enable='between(t,start,end)'`, trimmed with `trim`/`setpts`; base is a black
    canvas spanning the full timeline.
  - audio: each `adelay=start` + `volume`, `amix` (reuse `mix_at_times_args`
    ideas).
  - images: overlay + enable (the existing overlay chain, generalized).
  - text: `drawtext` or burned SRT for subtitle-style clips.
  This is the largest piece — staged and unit-tested (the arg builders are pure,
  like the current ones).

## 5. Frontend

- Timeline (`TimelineEditor`) reads `dub_clips` (not `buildClips`-from-segments).
  Drag/trim/restack → persist the clip. (We already have the react-timeline-editor
  surface + `onClipTrim` plumbing.)
- **Add media**: a real picker/drop → upload → create clip on a new lane.
- **Per-clip inspector**: position/size/opacity (visual), volume/trim (a/v), text
  style (text).
- **Compositing preview**: render the clips active at the playhead — stacked
  `<video>`/`<img>` layers positioned per clip + `<audio>` elements, driven by the
  transport. (Multi-video sync is the hard part; Phase 3 may start with "active
  video layer + image/text overlays" and add canvas compositing later.)

## 6. Phasing (each phase ships + is verifiable)

- **Phase 0 — Foundation**: `dub_clips` table + CRUD + a compose step that writes
  `dub:*` clips from current dub data; timeline reads clips (parity with today).
- **Phase 1 — Add + free-move**: upload media → clip; drag/trim/restack persists;
  per-clip inspector. (Reuses the existing timeline drag + overlay upload.)
- **Phase 2 — Compositing export**: generalized ffmpeg builder from clips;
  replaces the dub-specific export. Unit-tested arg builders.
- **Phase 3 — Compositing preview**: layered preview driven by the transport.
- **Phase 4 — Polish**: snapping, multi-select, transitions, perf.

## 7. Risks / open decisions

- **Browser preview compositing** (multiple synced `<video>`) is hard — may accept
  a limited preview first (one active video layer + overlays), full canvas later.
- **ffmpeg compositing** complexity + render time for many clips.
- **Backward compat**: existing dub projects keep working; the compose step
  back-fills clips on first open. The interim `video_offset_s`/segment `offset_s`
  become the seed for clip `start_s`, then are superseded.
- **Scope of "audio"**: original audio as part of the video clip vs. its own
  movable clip (start with part-of-video; split out later if needed).

## 8. Not in scope (initial)

Keyframe animation, effects/filters beyond opacity, color grading, speed ramps,
nested sequences. These can layer on once the clip model + compositor exist.
