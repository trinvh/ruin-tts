-- Render options for the dubbing export: burn the Vietnamese subtitles into the
-- video, and/or blur a band of the source video to cover hard-coded original
-- subtitles. The blur band is expressed as fractions of the video height
-- (default: a strip across the bottom where burned-in subs usually sit).

ALTER TABLE dub_projects ADD COLUMN burn_subtitles INTEGER NOT NULL DEFAULT 0;
ALTER TABLE dub_projects ADD COLUMN blur_subtitle  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE dub_projects ADD COLUMN blur_y         REAL NOT NULL DEFAULT 0.84;
ALTER TABLE dub_projects ADD COLUMN blur_h         REAL NOT NULL DEFAULT 0.14;
