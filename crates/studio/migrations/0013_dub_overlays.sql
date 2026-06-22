-- Image/banner overlays for the dubbing editor. Each overlay is an uploaded
-- image placed over the video for a time range, at a fractional position/size
-- (resolution-independent). Applied in preview AND burned into the export via
-- ffmpeg `overlay=...:enable='between(t,start,end)'`.

CREATE TABLE IF NOT EXISTS dub_overlays (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  file        TEXT NOT NULL,                 -- absolute path to the image on disk
  start_s     REAL NOT NULL DEFAULT 0,
  end_s       REAL NOT NULL DEFAULT 0,       -- <= start_s ⇒ shown for the whole video
  x           REAL NOT NULL DEFAULT 0.05,    -- top-left, fraction of frame width
  y           REAL NOT NULL DEFAULT 0.05,    -- top-left, fraction of frame height
  w           REAL NOT NULL DEFAULT 0.3,     -- width, fraction of frame (height keeps aspect)
  opacity     REAL NOT NULL DEFAULT 1,
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_dub_overlays_project ON dub_overlays(project_id);
