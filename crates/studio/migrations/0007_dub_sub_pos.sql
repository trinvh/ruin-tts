-- Vertical position of the burned Vietnamese subtitles, as a fraction of the
-- video height (0 = top, 1 = bottom). Default near the bottom.
ALTER TABLE dub_projects ADD COLUMN sub_y REAL NOT NULL DEFAULT 0.9;
