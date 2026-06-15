//! Read-only client for the Ruin API. Unwraps the {success,data} envelope and
//! authenticates with the X-API-Key header on every request. Verified against
//! the live service.

use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub meta: Meta,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
    #[serde(rename = "totalPages")]
    pub total_pages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Novel {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub original_title: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    pub status: String,
    pub chapter_count: u32,
    #[serde(default)]
    pub genres: Vec<Genre>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genre {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub id: String,
    pub number: u32,
    pub volume: Option<u32>,
    pub title: String,
    pub word_count: u32,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterContent {
    pub id: String,
    pub number: u32,
    pub volume: Option<u32>,
    pub title: String,
    pub content: String,
    pub word_count: u32,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    data: Option<T>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

/// Parse the standard envelope and return the inner `data`, or the API error.
pub fn parse_envelope<T: DeserializeOwned>(body: &str) -> Result<T> {
    let env: Envelope<T> = serde_json::from_str(body).context("decode Ruin envelope")?;
    if env.success {
        env.data
            .ok_or_else(|| anyhow!("Ruin response missing data"))
    } else {
        Err(anyhow!(
            "Ruin API error: {}",
            env.error
                .map(|e| e.message)
                .unwrap_or_else(|| "unknown".into())
        ))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChapterParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub order_asc: bool,
}

pub struct RuinClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl RuinClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            http: reqwest::Client::new(),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let res = self
            .http
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .header("accept", "application/json")
            .query(query)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let body = res
            .text()
            .await
            .with_context(|| format!("read body {url}"))?;
        parse_envelope::<T>(&body).with_context(|| format!("for {path}"))
    }

    pub async fn list_novels(
        &self,
        search: Option<&str>,
        page: u32,
        limit: u32,
    ) -> Result<Page<Novel>> {
        let mut q = vec![("page", page.to_string()), ("limit", limit.to_string())];
        if let Some(s) = search {
            q.push(("search", s.to_string()));
        }
        self.get("/novels", &q).await
    }

    pub async fn get_novel(&self, slug: &str) -> Result<Novel> {
        self.get(&format!("/novels/{slug}"), &[]).await
    }

    pub async fn list_chapters(&self, slug: &str, p: ChapterParams) -> Result<Page<Chapter>> {
        self.get(&format!("/novels/{slug}/chapters"), &chapter_query(p))
            .await
    }

    pub async fn chapters_content(
        &self,
        slug: &str,
        p: ChapterParams,
    ) -> Result<Page<ChapterContent>> {
        self.get(
            &format!("/novels/{slug}/chapters/content"),
            &chapter_query(p),
        )
        .await
    }

    pub async fn chapter(&self, slug: &str, number: u32) -> Result<ChapterContent> {
        self.get(&format!("/novels/{slug}/chapters/{number}"), &[])
            .await
    }
}

fn chapter_query(p: ChapterParams) -> Vec<(&'static str, String)> {
    let mut q = Vec::new();
    if let Some(page) = p.page {
        q.push(("page", page.to_string()));
    }
    if let Some(limit) = p.limit {
        q.push(("limit", limit.to_string()));
    }
    q.push((
        "order",
        if p.order_asc { "asc" } else { "desc" }.to_string(),
    ));
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_success_page() {
        let body = r#"{"success":true,"data":{"items":[{"id":"1","slug":"a","title":"T","originalTitle":null,"author":null,"coverUrl":null,"status":"ongoing","chapterCount":5,"genres":[]}],"meta":{"page":1,"limit":20,"total":1,"totalPages":1}}}"#;
        let page: Page<Novel> = parse_envelope(body).unwrap();
        assert_eq!(page.items[0].slug, "a");
        assert_eq!(page.items[0].chapter_count, 5);
        assert_eq!(page.meta.total, 1);
    }

    #[test]
    fn surfaces_api_error() {
        let body = r#"{"success":false,"error":{"message":"not found"}}"#;
        let r: Result<Novel> = parse_envelope(body);
        assert!(r.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn chapter_query_includes_order() {
        let q = chapter_query(ChapterParams {
            page: Some(1),
            limit: Some(2000),
            order_asc: true,
        });
        assert!(q.contains(&("order", "asc".to_string())));
        assert!(q.contains(&("limit", "2000".to_string())));
    }
}
