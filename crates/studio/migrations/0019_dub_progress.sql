-- Fine-grained progress for long dubbing steps (synthesize/translate/…). The UI
-- polls the project; these two columns let it show a real bar + what it's doing
-- instead of an indeterminate spinner. `progress` is 0..1, NULL = indeterminate
-- (the step is running but has no countable units, e.g. analyze). Both reset to
-- NULL on every status change (see set_dub_status).
ALTER TABLE dub_projects ADD COLUMN progress REAL;
ALTER TABLE dub_projects ADD COLUMN progress_label TEXT;
