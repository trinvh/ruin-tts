-- Per-project Vietnamese dub track volume (0..1), applied when muxing the
-- export. Defaults to full so existing projects are unchanged.
ALTER TABLE dub_projects ADD COLUMN vn_volume REAL NOT NULL DEFAULT 1.0;
