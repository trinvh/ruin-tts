-- Tool-owned state. Ruin is read-only; selection/queue/idempotency live here.

CREATE TABLE IF NOT EXISTS selections (
  slug       TEXT PRIMARY KEY,
  title      TEXT NOT NULL,
  cursor     INTEGER NOT NULL DEFAULT 0,   -- last produced chapter number
  enabled    INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Idempotency / resume: one row per produced (novel, range, workflow, content).
CREATE TABLE IF NOT EXISTS outputs (
  output_key       TEXT PRIMARY KEY,
  novel_slug       TEXT NOT NULL,
  first_chapter    INTEGER NOT NULL,
  last_chapter     INTEGER NOT NULL,
  workflow_version INTEGER NOT NULL,
  content_hash     TEXT NOT NULL,
  status           TEXT NOT NULL,           -- rendered | uploaded
  video_id         TEXT,
  created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_outputs_novel ON outputs (novel_slug);

CREATE TABLE IF NOT EXISTS jobs (
  id            TEXT PRIMARY KEY,
  novel_slug    TEXT NOT NULL,
  first_chapter INTEGER NOT NULL,
  last_chapter  INTEGER NOT NULL,
  status        TEXT NOT NULL,              -- queued | running | done | failed
  error         TEXT,
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs (status);

CREATE TABLE IF NOT EXISTS workflows (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  version    INTEGER NOT NULL,
  graph      TEXT NOT NULL,                 -- JSON WorkflowDef
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS profiles (
  name TEXT PRIMARY KEY,
  json TEXT NOT NULL                        -- JSON Profile
);

CREATE TABLE IF NOT EXISTS assets (
  id   TEXT PRIMARY KEY,
  kind TEXT NOT NULL,                       -- voice | intro_music | bg_music | background
  name TEXT NOT NULL,
  path TEXT NOT NULL
);
