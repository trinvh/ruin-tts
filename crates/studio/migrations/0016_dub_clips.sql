-- General clip-based timeline (Phase 0 foundation). One row per clip across all
-- tracks (video/audio/image/text). The dubbing pipeline populates `dub:*` clips
-- via the compose step; user-added clips carry origin='user' and are never
-- regenerated. Geometry/timing is stored per clip (never per-project).

CREATE TABLE IF NOT EXISTS dub_clips (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  track       INTEGER NOT NULL DEFAULT 0,
  kind        TEXT NOT NULL,            -- 'video' | 'audio' | 'image' | 'text'
  source      TEXT,                     -- file path; NULL for text
  start_s     REAL NOT NULL DEFAULT 0,
  dur_s       REAL NOT NULL DEFAULT 0,
  in_s        REAL NOT NULL DEFAULT 0,
  volume      REAL NOT NULL DEFAULT 1,
  x           REAL NOT NULL DEFAULT 0,
  y           REAL NOT NULL DEFAULT 0,
  w           REAL NOT NULL DEFAULT 1,
  opacity     REAL NOT NULL DEFAULT 1,
  text        TEXT,
  text_style  TEXT,
  origin      TEXT NOT NULL DEFAULT 'user',  -- 'user' | 'dub:video' | 'dub:tts:<segId>' | 'dub:sub:<segId>' | 'dub:banner:<overlayId>'
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_dub_clips_project ON dub_clips(project_id);
