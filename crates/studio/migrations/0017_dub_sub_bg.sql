-- Background box behind the Vietnamese subtitle (matches the preview's box).
ALTER TABLE dub_projects ADD COLUMN sub_bg INTEGER NOT NULL DEFAULT 1;
