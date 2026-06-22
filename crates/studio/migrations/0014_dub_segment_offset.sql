-- Per-segment time offset for the free-move timeline. When the operator drags a
-- dubbed line on the timeline, its audio + subtitle shift by this many seconds in
-- the built track and the exported video (duration is unchanged).
ALTER TABLE dub_segments ADD COLUMN offset_s REAL NOT NULL DEFAULT 0;
