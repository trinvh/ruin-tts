//! YouTube Data API v3 upload (OAuth refresh-token flow + resumable upload).
//! Credentials come from config/env; when absent the upload step is skipped.

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

pub struct YouTube {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct VideoMeta {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub privacy: String, // "private" | "unlisted" | "public"
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
}

#[derive(Deserialize)]
struct VideoResource {
    id: String,
}

impl YouTube {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            refresh_token: refresh_token.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Build from env vars (returns None if any are missing).
    pub fn from_env() -> Option<Self> {
        let id = std::env::var("YT_CLIENT_ID").ok()?;
        let secret = std::env::var("YT_CLIENT_SECRET").ok()?;
        let refresh = std::env::var("YT_REFRESH_TOKEN").ok()?;
        Some(Self::new(id, secret, refresh))
    }

    async fn access_token(&self) -> Result<String> {
        let resp: TokenResp = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("refresh YouTube token")?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.access_token)
    }

    /// Upload a video file; returns the YouTube video id.
    pub async fn upload(&self, path: &Path, meta: &VideoMeta) -> Result<String> {
        let token = self.access_token().await?;
        let body = json!({
            "snippet": { "title": meta.title, "description": meta.description, "tags": meta.tags },
            "status": { "privacyStatus": meta.privacy, "selfDeclaredMadeForKids": false }
        });
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("read {}", path.display()))?;

        // 1) Start a resumable session.
        let start = self
            .http
            .post("https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status")
            .bearer_auth(&token)
            .header("X-Upload-Content-Type", "video/*")
            .header("X-Upload-Content-Length", bytes.len().to_string())
            .json(&body)
            .send()
            .await
            .context("start resumable upload")?
            .error_for_status()?;
        let location = start
            .headers()
            .get("location")
            .ok_or_else(|| anyhow!("no resumable upload URL returned"))?
            .to_str()?
            .to_string();

        // 2) Upload the bytes (single PUT; chunking can be added for huge files).
        let resource: VideoResource = self
            .http
            .put(&location)
            .bearer_auth(&token)
            .header("content-type", "video/*")
            .body(bytes)
            .send()
            .await
            .context("upload video bytes")?
            .error_for_status()?
            .json()
            .await?;
        Ok(resource.id)
    }
}
