//! ruin-studio — webnovel → TTS audiobook → YouTube automation.
//!
//! Reads chapters from the Ruin API, narrates them through `vieneu-server`,
//! assembles audio + video with ffmpeg, and uploads to YouTube. The pipeline is
//! an operator-defined node graph (n8n-style). This crate owns its DB, queue,
//! and state; Ruin is read-only.

pub mod config;
pub mod db;
pub mod idempotency;
pub mod media;
pub mod nodes;
pub mod packing;
pub mod ruin;
pub mod server;
pub mod template;
pub mod tts;
pub mod workflow;
pub mod youtube;
