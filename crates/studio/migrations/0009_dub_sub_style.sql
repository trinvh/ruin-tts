-- Burned-subtitle style for the export: font size (px), colour (#RRGGBB hex), and
-- whether to render the source text above the Vietnamese (bilingual). Defaults
-- keep existing projects rendering as before (white, ~30px, Vietnamese only).
ALTER TABLE dub_projects ADD COLUMN sub_size REAL NOT NULL DEFAULT 30;
ALTER TABLE dub_projects ADD COLUMN sub_color TEXT NOT NULL DEFAULT '#ffffff';
ALTER TABLE dub_projects ADD COLUMN sub_bilingual INTEGER NOT NULL DEFAULT 0;
