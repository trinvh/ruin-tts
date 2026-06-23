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
    // Sampling — lower temperature keeps the voice consistent across sentences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repetition_penalty: Option<f32>,
    /// Silence (seconds) inserted between segments — raise it for a slower,
    /// storytelling pace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_p: Option<f32>,
    /// Silence (seconds) at paragraph boundaries (≥ silence_p).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_silence_p: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct JobCreated {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct CloneResponse {
    ref_id: String,
}

#[derive(Debug, Deserialize)]
struct JobView {
    status: String,
    ready: bool,
    #[serde(default)]
    error: Option<String>,
}

/// A voice offered by vieneu-server (`GET /v1/voices`). The label/name carries a
/// gender hint ("nam"/"nữ") used to auto-map dubbing speakers.
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    #[serde(default)]
    pub label: String,
}

/// Cache key for a rendered narration. Distinct chapter/voice/version/sampling/
/// text → distinct key, so edited chapters (or changed sampling) re-render but
/// unchanged ones are reused.
pub fn cache_key(
    chapter_id: &str,
    voice: &str,
    version: u32,
    sampling: &str,
    text: &str,
) -> String {
    content_hash(&[chapter_id, voice, &version.to_string(), sampling, text])
}

/// A stable fingerprint of a request's sampling params (for the cache key).
fn sampling_sig(req: &SynthRequest) -> String {
    format!(
        "t{:?}|k{:?}|p{:?}|r{:?}|s{:?}|ps{:?}",
        req.temperature,
        req.top_k,
        req.top_p,
        req.repetition_penalty,
        req.silence_p,
        req.paragraph_silence_p
    )
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

    /// Retry a request-producing closure a few times on transient errors
    /// (connection drops, partial bodies) with linear backoff. The TTS server
    /// can briefly drop connections under load or restart; one blip shouldn't
    /// fail a 20-minute render.
    async fn with_retry<T, F, Fut>(&self, what: &str, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<T>>,
    {
        let mut last: Option<reqwest::Error> = None;
        for attempt in 0..3u32 {
            match f().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::warn!(what, attempt = attempt + 1, "TTS request failed: {e}");
                    last = Some(e);
                    tokio::time::sleep(Duration::from_millis(300 * (attempt as u64 + 1))).await;
                }
            }
        }
        Err(anyhow::Error::new(last.expect("retry ran at least once"))
            .context(format!("{what} (sau khi thử lại)")))
    }

    /// Register a cloned voice from a reference WAV (`POST /v1/clone`), returning
    /// the server-side `ref_id` to use in synthesis. The handle lives in the TTS
    /// server's memory until it restarts, so callers should cache + re-clone.
    pub async fn clone_voice(&self, wav: Vec<u8>) -> Result<String> {
        let url = format!("{}/v1/clone", self.base_url);
        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("ref.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::multipart::Form::new().part("file", part);
        let resp: CloneResponse = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.ref_id)
    }

    /// List the voices offered by the server (`GET /v1/voices`).
    pub async fn list_voices(&self) -> Result<Vec<VoiceInfo>> {
        let url = format!("{}/v1/voices", self.base_url);
        let voices = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<VoiceInfo>>()
            .await?;
        Ok(voices)
    }

    /// Synthesize, returning the audio bytes (waits for the job to finish).
    pub async fn synth(&self, req: &SynthRequest) -> Result<Vec<u8>> {
        let jobs_url = format!("{}/v1/jobs", self.base_url);
        let created: JobCreated = self
            .with_retry("POST /v1/jobs", || async {
                self.http
                    .post(&jobs_url)
                    .json(req)
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<JobCreated>()
                    .await
            })
            .await?;

        let view_url = format!("{}/v1/jobs/{}", self.base_url, created.job_id);
        let mut poll_errors = 0u32;
        loop {
            tokio::time::sleep(self.poll).await;
            let view: JobView = match async {
                self.http
                    .get(&view_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<JobView>()
                    .await
            }
            .await
            {
                Ok(v) => {
                    poll_errors = 0;
                    v
                }
                // Tolerate transient poll failures (server blip) up to a bound.
                Err(e) => {
                    poll_errors += 1;
                    if poll_errors > 20 {
                        return Err(anyhow::Error::new(e).context("theo dõi tiến trình TTS"));
                    }
                    tracing::warn!(job = %created.job_id, poll_errors, "transient TTS poll error: {e}");
                    continue;
                }
            };
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

        let dl_url = format!("{}/v1/jobs/{}/download", self.base_url, created.job_id);
        let bytes = self
            .with_retry("download", || async {
                self.http
                    .get(&dl_url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await
            })
            .await?;
        Ok(bytes.to_vec())
    }

    /// Synthesize with caching; returns the path to the cached audio file.
    ///
    /// `cache_voice` is the STABLE voice label used in the cache key — pass the
    /// preset name or a `clone:<id>` handle, not the volatile per-session `ref_id`
    /// (which changes every run and would defeat the cache).
    pub async fn synth_cached(
        &self,
        cache_dir: &Path,
        chapter_id: &str,
        version: u32,
        req: &SynthRequest,
        cache_voice: &str,
        force: bool,
    ) -> Result<PathBuf> {
        let key = cache_key(
            chapter_id,
            cache_voice,
            version,
            &sampling_sig(req),
            &req.text,
        );
        let path = cache_dir.join(format!("{key}.{}", req.format));
        // `force` regenerates even when a cached file exists (overwriting it).
        if !force && path.exists() {
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
        let s = "t0.5";
        let a = cache_key("ch1", "Bình An", 1, s, "xin chào");
        assert_eq!(a, cache_key("ch1", "Bình An", 1, s, "xin chào"));
        assert_ne!(a, cache_key("ch1", "Bình An", 2, s, "xin chào")); // version
        assert_ne!(a, cache_key("ch1", "Ngọc Linh", 1, s, "xin chào")); // voice
        assert_ne!(a, cache_key("ch2", "Bình An", 1, s, "xin chào")); // chapter
        assert_ne!(a, cache_key("ch1", "Bình An", 1, s, "khác")); // text (edit)
        assert_ne!(a, cache_key("ch1", "Bình An", 1, "t0.8", "xin chào")); // sampling
    }
}
