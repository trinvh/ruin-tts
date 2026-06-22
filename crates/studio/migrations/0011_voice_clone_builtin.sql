-- Built-in voice-pack support for voice_clones. Bundled CC-BY voices are seeded
-- on startup (see crates/studio/src/clones/seed.rs) as rows with builtin = 1;
-- they cannot be renamed or deleted, and carry attribution required by CC-BY.

ALTER TABLE voice_clones ADD COLUMN builtin     INTEGER NOT NULL DEFAULT 0;
ALTER TABLE voice_clones ADD COLUMN source      TEXT;   -- e.g. "LSVSC (doof-ferb)"
ALTER TABLE voice_clones ADD COLUMN license     TEXT;   -- e.g. "CC BY 4.0"
ALTER TABLE voice_clones ADD COLUMN source_url  TEXT;   -- dataset URL for attribution
