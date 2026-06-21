-- Video dubbing projects: import a video, analyse it (ASR + diarization +
-- gender), translate to Vietnamese, synthesize per-segment TTS, fit to the
-- source timing, then preview/export over the original video.

CREATE TABLE IF NOT EXISTS dub_projects (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL DEFAULT '',
  video_path      TEXT NOT NULL,
  audio_path      TEXT,                       -- extracted 16k mono wav
  status          TEXT NOT NULL DEFAULT 'created',
  error           TEXT,
  language        TEXT,                       -- detected source language
  gemini_model    TEXT NOT NULL DEFAULT 'gemini-2.5-flash',
  original_volume REAL NOT NULL DEFAULT 0.15, -- original audio gain under the dub
  speed_cap       REAL NOT NULL DEFAULT 1.5,  -- max atempo before "translate shorter"
  vn_track_path   TEXT,                       -- assembled Vietnamese track
  export_path     TEXT,                       -- final muxed video
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_dub_projects_status ON dub_projects (status);

-- One transcribed/translated line.
CREATE TABLE IF NOT EXISTS dub_segments (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  idx         INTEGER NOT NULL,
  start_s     REAL NOT NULL,
  end_s       REAL NOT NULL,
  speaker     TEXT NOT NULL DEFAULT '',
  text_src    TEXT NOT NULL DEFAULT '',
  text_vi     TEXT NOT NULL DEFAULT '',
  voice       TEXT,                           -- resolved per-segment voice
  tts_path    TEXT,
  fitted_path TEXT,                           -- after atempo time-fit
  factor      REAL,                           -- applied tempo factor
  status      TEXT NOT NULL DEFAULT 'pending',
  FOREIGN KEY (project_id) REFERENCES dub_projects(id)
);
CREATE INDEX IF NOT EXISTS idx_dub_segments_project ON dub_segments (project_id, idx);

-- One detected speaker, its (best-effort) gender/age, and the assigned voice.
CREATE TABLE IF NOT EXISTS dub_speakers (
  project_id  TEXT NOT NULL,
  speaker     TEXT NOT NULL,
  gender      TEXT,
  age         REAL,
  voice       TEXT,
  PRIMARY KEY (project_id, speaker)
);
