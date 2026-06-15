//! TTS client — narrates text by driving the `vieneu-server` jobs API, with a
//! on-disk cache keyed by (chapterId, voice, version, text) so re-renders are
//! cheap.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::idempotency::content_hash;

#[derive(Debug, Clone, Serialize)]
pub struct SynthRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    pub emotion: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
struct JobCreated {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct JobView {
    status: String,
    ready: bool,
    #[serde(default)]
    error: Option<String>,
}

/// Cache key for a rendered narration. Distinct chapter/voice/version/text →
/// distinct key, so edited chapters re-render but unchanged ones are reused.
pub fn cache_key(chapter_id: &str, voice: &str, version: u32, text: &str) -> String {
    content_hash(&[chapter_id, voice, &version.to_string(), text])
}

pub struct TtsClient {
    base_url: String,
    http: reqwest::Client,
    poll: Duration,
}

impl TtsClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            poll: Duration::from_millis(500),
        }
    }

    /// Synthesize, returning the audio bytes (waits for the job to finish).
    pub async fn synth(&self, req: &SynthRequest) -> Result<Vec<u8>> {
        let created: JobCreated = self
            .http
            .post(format!("{}/v1/jobs", self.base_url))
            .json(req)
            .send()
            .await
            .context("POST /v1/jobs")?
            .error_for_status()?
            .json()
            .await?;

        loop {
            tokio::time::sleep(self.poll).await;
            let view: JobView = self
                .http
                .get(format!("{}/v1/jobs/{}", self.base_url, created.job_id))
                .send()
                .await?
                .json()
                .await?;
            match view.status.as_str() {
                "done" if view.ready => break,
                "failed" => {
                    return Err(anyhow!(
                        "TTS job failed: {}",
                        view.error.unwrap_or_default()
                    ))
                }
                "cancelled" => return Err(anyhow!("TTS job cancelled")),
                _ => continue,
            }
        }

        let bytes = self
            .http
            .get(format!(
                "{}/v1/jobs/{}/download",
                self.base_url, created.job_id
            ))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(bytes.to_vec())
    }

    /// Synthesize with caching; returns the path to the cached audio file.
    pub async fn synth_cached(
        &self,
        cache_dir: &Path,
        chapter_id: &str,
        version: u32,
        req: &SynthRequest,
    ) -> Result<PathBuf> {
        let voice = req
            .voice
            .as_deref()
            .or(req.ref_id.as_deref())
            .unwrap_or("default");
        let key = cache_key(chapter_id, voice, version, &req.text);
        let path = cache_dir.join(format!("{key}.{}", req.format));
        if path.exists() {
            return Ok(path);
        }
        let bytes = self.synth(req).await?;
        tokio::fs::create_dir_all(cache_dir).await.ok();
        tokio::fs::write(&path, &bytes)
            .await
            .with_context(|| format!("write cache {}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable_and_discriminating() {
        let a = cache_key("ch1", "Bình An", 1, "xin chào");
        assert_eq!(a, cache_key("ch1", "Bình An", 1, "xin chào"));
        assert_ne!(a, cache_key("ch1", "Bình An", 2, "xin chào")); // version
        assert_ne!(a, cache_key("ch1", "Ngọc Linh", 1, "xin chào")); // voice
        assert_ne!(a, cache_key("ch2", "Bình An", 1, "xin chào")); // chapter
        assert_ne!(a, cache_key("ch1", "Bình An", 1, "khác")); // text (edit)
    }
}
