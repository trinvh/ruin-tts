-- Video lead-in: seconds of empty space before the source video starts on the
-- timeline. Dragging the video clip later sets this; the export pads the video
-- with black + delays the audio + shifts subtitles by the same amount, leaving
-- room at the front for a title/banner.

ALTER TABLE dub_projects ADD COLUMN video_offset_s REAL NOT NULL DEFAULT 0;
