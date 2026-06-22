-- Per-track "deleted/disabled" state for the Video Studio editor. The other
-- tracks already map to existing fields (original_volume/vn_volume = audio on,
-- burn_subtitles = VN subtitle, sub_bilingual = source subtitle); the video
-- track needs its own flag. When off, export produces an audio-only file.

ALTER TABLE dub_projects ADD COLUMN video_enabled INTEGER NOT NULL DEFAULT 1;
