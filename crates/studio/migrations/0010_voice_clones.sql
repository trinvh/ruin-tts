-- Voice clones: a short WAV sample uploaded by the user, stored on disk under
-- <work_dir>/clones/<id>.wav. The TTS engine (vieneu-server /v1/clone) accepts
-- the bytes each session to obtain a ref_id; that exchange is done client-side.

CREATE TABLE IF NOT EXISTS voice_clones (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  file        TEXT NOT NULL   -- absolute path to the WAV on disk
);
