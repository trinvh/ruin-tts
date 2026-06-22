-- Burning the Vietnamese subtitle into the video is the expected default;
-- turn it on for existing projects (it can still be toggled off per project).
UPDATE dub_projects SET burn_subtitles = 1 WHERE burn_subtitles = 0;
