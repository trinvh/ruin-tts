//! HTTP clients for the dubbing pipeline: the media-ai analysis sidecar and the
//! Gemini translation API.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

// ── media-ai sidecar ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnalyzeRequest<'a> {
    audio_path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint_lang: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_speakers: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeResponse {
    pub language: String,
    pub segments: Vec<AnalyzedSegment>,
    pub speakers: Vec<AnalyzedSpeaker>,
    /// Overlapping-speech spans with per-speaker transcripts (source separation).
    #[serde(default)]
    pub overlaps: Vec<AnalyzedOverlap>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzedOverlap {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub texts: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzedSegment {
    pub id: i64,
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text_src: String,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzedSpeaker {
    pub speaker: String,
    pub gender: Option<String>,
    pub age: Option<f64>,
}

pub struct MediaAiClient {
    base_url: String,
    http: reqwest::Client,
}

impl MediaAiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            // Analysis (whisper + diarization) on a few-minute clip can take a
            // while; give it room rather than timing out mid-run.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(1800))
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn analyze(
        &self,
        audio_path: &str,
        hint_lang: Option<&str>,
        num_speakers: Option<u32>,
    ) -> Result<AnalyzeResponse> {
        let url = format!("{}/analyze", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&AnalyzeRequest {
                audio_path,
                hint_lang,
                num_speakers,
            })
            .send()
            .await
            .context("gọi media-ai /analyze (sidecar đang chạy chưa?)")?;
        if !resp.status().is_success() {
            let code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("media-ai lỗi {code}: {body}"));
        }
        resp.json::<AnalyzeResponse>()
            .await
            .context("đọc kết quả media-ai")
    }
}

// ── Gemini translation ───────────────────────────────────────────────────────────

/// One line to translate, with the speaker label and an optional max-word budget
/// (used by the "translate shorter" retry so the Vietnamese fits the time slot).
#[derive(Debug, Clone)]
pub struct TranslateLine {
    pub id: i64,
    pub speaker: String,
    pub text: String,
    /// Slot duration in seconds (shown to the model so it fits the timeline).
    pub seconds: f64,
    pub max_words: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TranslatedLine {
    id: i64,
    vi: String,
}

pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl GeminiClient {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Translate a batch of lines to Vietnamese in one call, preserving ids.
    /// Translates with cross-line context (the whole batch is in the prompt) so
    /// pronouns/register stay consistent — important for Vietnamese address terms.
    pub async fn translate(
        &self,
        source_lang: &str,
        lines: &[TranslateLine],
    ) -> Result<Vec<(i64, String)>> {
        if lines.is_empty() {
            return Ok(vec![]);
        }
        let prompt = build_prompt(source_lang, lines);
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        // responseSchema forces Gemini to emit strictly-typed JSON, which avoids
        // the occasional malformed object (e.g. `"id 22,` missing the `":`).
        //
        // thinkingBudget=0 disables the Gemini 2.5 "thinking" pass. It is on by
        // default for 2.5-flash and adds large latency for no quality gain on a
        // constrained dialogue-translation task — the main cause of slow runs.
        // (2.5-pro ignores 0 and keeps a minimum budget; harmless there.)
        let body = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": {
                "responseMimeType": "application/json",
                "responseSchema": {
                    "type": "ARRAY",
                    "items": {
                        "type": "OBJECT",
                        "properties": {
                            "id": { "type": "INTEGER" },
                            "vi": { "type": "STRING" }
                        },
                        "required": ["id", "vi"]
                    }
                },
                "temperature": 0.3,
                "thinkingConfig": { "thinkingBudget": 0 }
            }
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("gọi Gemini API")?;
        if !resp.status().is_success() {
            let code = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Gemini lỗi {code}: {text}"));
        }
        let v: serde_json::Value = resp.json().await.context("đọc phản hồi Gemini")?;
        let text = v
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("Gemini không trả về nội dung: {v}"))?;
        let parsed = parse_translations(text)
            .with_context(|| format!("phân tích JSON dịch từ Gemini: {text}"))?;
        Ok(parsed.into_iter().map(|l| (l.id, l.vi)).collect())
    }
}

/// Parse Gemini's translation array, tolerating two common defects: a markdown
/// code fence around the JSON, and a malformed key `"id <n>` (missing `":`).
fn parse_translations(text: &str) -> Result<Vec<TranslatedLine>> {
    let trimmed = strip_code_fence(text);
    if let Ok(v) = serde_json::from_str::<Vec<TranslatedLine>>(trimmed) {
        return Ok(v);
    }
    // Repair the observed defect: `"id 22,` → `"id": 22,`. Valid `"id":` has a
    // quote (not a space) after `d`, so it is left untouched.
    let repaired = trimmed.replace("\"id ", "\"id\": ");
    serde_json::from_str::<Vec<TranslatedLine>>(&repaired).map_err(|e| anyhow!("{e}"))
}

fn strip_code_fence(text: &str) -> &str {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_and_malformed_and_fenced() {
        let clean = r#"[{"id":0,"vi":"Xin chào"},{"id":1,"vi":"Tạm biệt"}]"#;
        assert_eq!(parse_translations(clean).unwrap().len(), 2);

        // the exact defect Gemini produced: `"id 22,` without the `":`
        let bad = r#"[{"id":0,"vi":"a"},{"id 22,"vi":"b"}]"#;
        let got = parse_translations(bad).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, 22);

        let fenced = "```json\n[{\"id\":3,\"vi\":\"c\"}]\n```";
        assert_eq!(parse_translations(fenced).unwrap()[0].id, 3);
    }
}

fn build_prompt(source_lang: &str, lines: &[TranslateLine]) -> String {
    let mut items = String::new();
    for l in lines {
        let budget = match l.max_words {
            Some(n) => format!(", tối đa {n} từ"),
            None => String::new(),
        };
        items.push_str(&format!(
            "- id {id} [{spk}, {sec:.1}s{budget}]: {text}\n",
            id = l.id,
            spk = l.speaker,
            sec = l.seconds,
            budget = budget,
            text = l.text.replace('\n', " ")
        ));
    }
    format!(
        "Bạn là dịch giả lồng tiếng chuyên nghiệp. Dịch các câu thoại sau (ngôn ngữ nguồn: {src}) \
sang TIẾNG VIỆT tự nhiên, đúng văn nói, phù hợp lồng tiếng phim.\n\n\
Mỗi câu có ghi [người nói, thời lượng giây, giới hạn từ]. RẤT QUAN TRỌNG: bản dịch phải đủ NGẮN \
để đọc vừa trong thời lượng đó (người Việt nói ~2.3 từ/giây). Ưu tiên gọn, tự nhiên hơn là dịch đầy đủ từng chữ.\n\n\
Quy tắc:\n\
- Tôn trọng giới hạn 'tối đa N từ' của từng câu.\n\
- Giữ nhất quán đại từ xưng hô giữa các nhân vật (theo nhãn người nói).\n\
- Nghe như người Việt nói, KHÔNG dịch máy móc; có thể lược bỏ từ thừa để vừa thời lượng.\n\
- KHÔNG thêm chú thích. Trả về DUY NHẤT một mảng JSON: [{{\"id\": <số>, \"vi\": \"<bản dịch>\"}}].\n\n\
Các câu cần dịch:\n{items}",
        src = source_lang,
        items = items
    )
}
