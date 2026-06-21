-- Upgrade the blur cover from a full-width band to a free rectangle: add the
-- horizontal position + width (fractions of video width). Defaults keep the old
-- bottom-band behaviour (x=0, w=1).
ALTER TABLE dub_projects ADD COLUMN blur_x REAL NOT NULL DEFAULT 0.0;
ALTER TABLE dub_projects ADD COLUMN blur_w REAL NOT NULL DEFAULT 1.0;
