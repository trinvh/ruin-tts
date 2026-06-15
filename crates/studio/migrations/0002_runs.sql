-- Persisted runs with per-node progress (so the UI can show live status,
-- inputs and outputs for every node).

CREATE TABLE IF NOT EXISTS runs (
  id          TEXT PRIMARY KEY,
  graph       TEXT NOT NULL,                -- JSON WorkflowDef executed
  status      TEXT NOT NULL,                -- queued | running | done | failed
  preview     INTEGER NOT NULL DEFAULT 0,
  label       TEXT NOT NULL DEFAULT '',
  error       TEXT,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_runs_status ON runs (status);

CREATE TABLE IF NOT EXISTS run_steps (
  run_id      TEXT NOT NULL,
  idx         INTEGER NOT NULL,
  node_id     TEXT NOT NULL,
  node_type   TEXT NOT NULL,
  status      TEXT NOT NULL,                -- pending | running | done | failed
  input_json  TEXT,
  output_json TEXT,
  started_at  TEXT,
  finished_at TEXT,
  PRIMARY KEY (run_id, node_id)
);
