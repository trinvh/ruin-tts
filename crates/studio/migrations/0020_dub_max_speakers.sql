-- Per-project override for the diarization speaker cap. NULL = inherit the
-- global default (AppConfig.dub_max_speakers, default 4). Without a ceiling
-- pyannote over-clusters long/noisy videos into hundreds of phantom speakers.
ALTER TABLE dub_projects ADD COLUMN max_speakers INTEGER;
