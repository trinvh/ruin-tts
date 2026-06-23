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

/// How many times to wait-and-retry a rate-limited / transient Gemini call
/// before giving up on that chunk.
const GEMINI_MAX_RETRIES: u32 = 6;

pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
    /// Set once any call is rate-limited (429). The chunk loop reads this to pace
    /// itself only on a throttled (free-tier) key — a pro key that never 429s
    /// stays un-paced.
    throttled: std::sync::atomic::AtomicBool,
}

/// Pull the suggested wait (seconds) out of a Gemini error body — the
/// `RetryInfo` detail carries `"retryDelay": "30s"`. Returns None if absent or
/// unparseable so the caller can fall back to its own backoff.
fn retry_delay_secs(body: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let details = v.get("error")?.get("details")?.as_array()?;
    for d in details {
        let is_retry_info = d
            .get("@type")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t.ends_with("RetryInfo"));
        if is_retry_info {
            if let Some(rd) = d.get("retryDelay").and_then(|r| r.as_str()) {
                // "30s" / "1.5s" → ceil to whole seconds, plus a small margin.
                if let Some(num) = rd.trim().strip_suffix('s') {
                    if let Ok(f) = num.parse::<f64>() {
                        return Some(f.ceil() as u64 + 1);
                    }
                }
            }
        }
    }
    None
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
            throttled: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Whether any call so far has been rate-limited — the chunk loop paces
    /// itself only once this is true (free-tier key).
    pub fn is_throttled(&self) -> bool {
        self.throttled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Translate one batch of lines to Vietnamese in a single call, preserving
    /// ids. `context` carries a few already-translated (source → vi) pairs from
    /// the preceding batch so pronouns/register stay consistent across chunk
    /// boundaries — important for Vietnamese address terms. The caller chunks a
    /// long script and feeds the rolling context (see `pipeline::translate`).
    ///
    /// Parsing is tolerant: a truncated response (Gemini hit its output cap on a
    /// very long batch) still yields every complete `{id, vi}` object, and the
    /// caller re-requests whatever ids came back missing.
    pub async fn translate(
        &self,
        source_lang: &str,
        lines: &[TranslateLine],
        context: &[(String, String)],
    ) -> Result<Vec<(i64, String)>> {
        if lines.is_empty() {
            return Ok(vec![]);
        }
        let prompt = build_prompt(source_lang, lines, context);
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
                // Plenty of room for one ~80-line chunk; the caller keeps chunks
                // small enough that this is rarely the binding limit, and salvage
                // + missing-id retry recover anything that still gets clipped.
                "maxOutputTokens": 8192,
                "thinkingConfig": { "thinkingBudget": 0 }
            }
        });
        // Free-tier keys hit per-minute quota easily now that a long script is
        // chunked into many calls. On 429 (RESOURCE_EXHAUSTED) — and transient
        // 503s — honour the server's suggested `retryDelay` and retry, instead of
        // failing the whole translate. Capped so a bad value can't hang the run.
        let v: serde_json::Value = {
            let mut attempt = 0u32;
            loop {
                let resp = self
                    .http
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .context("gọi Gemini API")?;
                let code = resp.status();
                if code.is_success() {
                    break resp.json().await.context("đọc phản hồi Gemini")?;
                }
                let text = resp.text().await.unwrap_or_default();
                let retryable = code.as_u16() == 429 || code.as_u16() == 503;
                if code.as_u16() == 429 {
                    // Free-tier quota hit — tell the chunk loop to start pacing.
                    self.throttled
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if retryable && attempt < GEMINI_MAX_RETRIES {
                    attempt += 1;
                    let wait = retry_delay_secs(&text)
                        .unwrap_or(30 * attempt as u64)
                        .clamp(1, 90);
                    tracing::warn!(
                        "Gemini {code} (lần {attempt}/{GEMINI_MAX_RETRIES}) — chờ {wait}s rồi thử lại"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    continue;
                }
                let hint = if retryable {
                    " (đã hết lượt thử lại — Gemini giới hạn tốc độ; chờ ít phút hoặc nâng cấp gói)"
                } else {
                    ""
                };
                return Err(anyhow!("Gemini lỗi {code}{hint}: {text}"));
            }
        };
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

/// Parse Gemini's translation array, tolerating three defects: a markdown code
/// fence around the JSON, a malformed key `"id <n>` (missing `":`), and a
/// TRUNCATED array — when the response hit the output-token cap mid-string, the
/// trailing broken object is dropped and every complete `{id, vi}` is kept. The
/// caller re-requests any ids that came back missing.
fn parse_translations(text: &str) -> Result<Vec<TranslatedLine>> {
    let trimmed = strip_code_fence(text);
    // Repair the observed defect: `"id 22,` → `"id": 22,`. Valid `"id":` has a
    // quote (not a space) after `d`, so it is left untouched.
    let repaired = trimmed.replace("\"id ", "\"id\": ");
    if let Ok(v) = serde_json::from_str::<Vec<TranslatedLine>>(&repaired) {
        return Ok(v);
    }
    // Strict parse failed (most often: truncated array). Salvage every complete
    // top-level object instead of losing the whole batch.
    let salvaged = salvage_objects(&repaired);
    if salvaged.is_empty() {
        // Nothing recoverable — surface the original text so the error is useful.
        return Err(anyhow!("không đọc được JSON dịch (rỗng/hỏng)"));
    }
    Ok(salvaged)
}

/// Scan a (possibly truncated) JSON array and parse each complete top-level
/// `{...}` object individually, respecting string escaping so a `}` inside a
/// translation never ends an object early. Incomplete trailing objects are
/// skipped. Tolerant by design: malformed individual objects are dropped, not
/// fatal.
fn salvage_objects(text: &str) -> Vec<TranslatedLine> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        // Walk to the matching close brace, tracking string state + escapes.
        let start = i;
        let mut depth = 0usize;
        let mut in_str = false;
        let mut esc = false;
        let mut end = None;
        let mut j = i;
        while j < bytes.len() {
            let c = bytes[j];
            if in_str {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == b'"' {
                    in_str = false;
                }
            } else if c == b'"' {
                in_str = true;
            } else if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = Some(j);
                    break;
                }
            }
            j += 1;
        }
        match end {
            Some(e) => {
                if let Ok(line) = serde_json::from_str::<TranslatedLine>(&text[start..=e]) {
                    out.push(line);
                }
                i = e + 1;
            }
            // Unterminated final object (truncation) — stop; keep what we have.
            None => break,
        }
    }
    out
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

    #[test]
    fn salvages_truncated_array() {
        // The exact failure shape: a long array clipped mid-string at the cap.
        let truncated = "[{\"id\": 2683, \"vi\": \"Ngươi cười cái gì?\"},\n\
             {\"id\": 2684, \"vi\": \"Ngươi cười cái gì?\"},\n\
             {\"id\": 2685, \"vi\": \"Khớp thời gian.\"},\n\
             {\"id\": 2686, \"vi";
        let got = parse_translations(truncated).unwrap();
        assert_eq!(got.len(), 3); // last broken object dropped, first three kept
        assert_eq!(got[0].id, 2683);
        assert_eq!(got[2].id, 2685);
    }

    #[test]
    fn reads_retry_delay_from_429_body() {
        let body = r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","details":[
            {"@type":"type.googleapis.com/google.rpc.QuotaFailure"},
            {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"30s"}
        ]}}"#;
        assert_eq!(retry_delay_secs(body), Some(31)); // 30 + 1s margin
        assert_eq!(retry_delay_secs("not json"), None);
        assert_eq!(retry_delay_secs(r#"{"error":{}}"#), None);
    }

    #[test]
    fn salvage_keeps_braces_inside_strings() {
        // A `}` inside a translation must not end the object early.
        let tricky = r#"[{"id":1,"vi":"a }} b"},{"id":2,"vi":"ok"}"#; // missing final ]
        let got = parse_translations(tricky).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, 2);
    }
}

fn build_prompt(
    source_lang: &str,
    lines: &[TranslateLine],
    context: &[(String, String)],
) -> String {
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
    // Rolling context from the previous chunk: already-translated lines shown so
    // address terms / register stay consistent across the chunk boundary. The
    // model must NOT re-translate or echo these — they are reference only.
    let context_block = if context.is_empty() {
        String::new()
    } else {
        let mut c = String::from(
            "Ngữ cảnh đoạn LIỀN TRƯỚC (đã dịch — chỉ để giữ nhất quán xưng hô/văn phong; \
KHÔNG dịch lại, KHÔNG đưa vào kết quả):\n",
        );
        for (src, vi) in context {
            c.push_str(&format!(
                "- {} → {}\n",
                src.replace('\n', " "),
                vi.replace('\n', " ")
            ));
        }
        c.push('\n');
        c
    };
    format!(
        "Bạn là dịch giả lồng tiếng chuyên nghiệp. Dịch các câu thoại sau (ngôn ngữ nguồn: {src}) \
sang TIẾNG VIỆT tự nhiên, đúng văn nói, phù hợp lồng tiếng phim.\n\n\
Mỗi câu có ghi [người nói, thời lượng giây, giới hạn từ]. RẤT QUAN TRỌNG: bản dịch phải đủ NGẮN \
để đọc vừa trong thời lượng đó (người Việt nói ~2.3 từ/giây). Ưu tiên gọn, tự nhiên hơn là dịch đầy đủ từng chữ.\n\n\
Quy tắc:\n\
- Tôn trọng giới hạn 'tối đa N từ' của từng câu.\n\
- Giữ nhất quán đại từ xưng hô giữa các nhân vật (theo nhãn người nói), kể cả với ngữ cảnh đoạn trước.\n\
- Nghe như người Việt nói, KHÔNG dịch máy móc; có thể lược bỏ từ thừa để vừa thời lượng.\n\
- Dịch ĐÚNG và ĐỦ mọi id được liệt kê bên dưới, không bỏ sót câu nào.\n\
- KHÔNG thêm chú thích. Trả về DUY NHẤT một mảng JSON: [{{\"id\": <số>, \"vi\": \"<bản dịch>\"}}].\n\n\
{context_block}Các câu cần dịch:\n{items}",
        src = source_lang,
        context_block = context_block,
        items = items
    )
}
