//! Operator configuration: global defaults + per-novel overrides (profiles) and
//! the templated text/assets a pipeline uses.

use serde::{Deserialize, Serialize};

use crate::media::DuckSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuckCfg {
    pub music_volume: f64,
    pub threshold: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
}

impl Default for DuckCfg {
    fn default() -> Self {
        let d = DuckSettings::default();
        Self {
            music_volume: d.music_volume,
            threshold: d.threshold,
            ratio: d.ratio,
            attack: d.attack,
            release: d.release,
        }
    }
}

impl From<&DuckCfg> for DuckSettings {
    fn from(c: &DuckCfg) -> Self {
        DuckSettings {
            music_volume: c.music_volume,
            threshold: c.threshold,
            ratio: c.ratio,
            attack: c.attack,
            release: c.release,
        }
    }
}

/// All operator settings — API keys, service URLs, and the render profile.
/// Persisted in the DB and edited from the app's Settings page (never env/CLI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub ruin_base: String,
    pub ruin_key: String,
    pub tts_base: String,
    pub yt_client_id: String,
    pub yt_client_secret: String,
    pub yt_refresh_token: String,
    pub yt_privacy: String,
    /// Video dubbing: analysis sidecar (media-ai) base URL.
    pub media_ai_base: String,
    /// Video dubbing: Gemini API key (translation) + model.
    pub gemini_api_key: String,
    pub gemini_model: String,
    /// Default voices used to auto-map dubbing speakers by detected gender
    /// (vieneu's voice list carries no gender, so the operator sets these once).
    pub dub_voice_male: String,
    pub dub_voice_female: String,
    pub profile: Profile,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ruin_base: "https://ruins-api.apps.trinvh.com/api/v1".into(),
            ruin_key: String::new(),
            tts_base: "http://127.0.0.1:8080".into(),
            yt_client_id: String::new(),
            yt_client_secret: String::new(),
            yt_refresh_token: String::new(),
            yt_privacy: "private".into(),
            media_ai_base: "http://127.0.0.1:8099".into(),
            gemini_api_key: String::new(),
            gemini_model: "gemini-2.5-flash".into(),
            dub_voice_male: String::new(),
            dub_voice_female: String::new(),
            profile: Profile::default(),
        }
    }
}

impl AppConfig {
    pub fn youtube_ready(&self) -> bool {
        !self.yt_client_id.is_empty()
            && !self.yt_client_secret.is_empty()
            && !self.yt_refresh_token.is_empty()
    }
}

/// A reusable rendering profile (global default; can be overridden per novel).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub site_name: String,
    pub voice: String,
    pub emotion: String,
    pub format: String,

    pub wpm: f64,
    pub cap_seconds: f64,
    pub overhead_seconds: f64,

    pub width: u32,
    pub height: u32,
    pub background_path: Option<String>,
    pub background_is_video: bool,
    pub intro_music_path: Option<String>,
    pub bg_music_path: Option<String>,
    pub duck: DuckCfg,

    pub intro_template: String,
    pub outro_template: String,
    pub title_template: String,
    pub description_template: String,
    pub tags_template: String,

    /// Silence (seconds) padding the spoken parts of each chunk:
    /// `<before_intro> intro <after_intro> content <after_content> outro <after_outro>`.
    pub delay_before_intro: f64,
    pub delay_after_intro: f64,
    pub delay_after_content: f64,
    pub delay_after_outro: f64,

    /// Narration sampling. Lower temperature keeps the voice consistent across
    /// sentences (each is generated independently); 0.8 (engine default) drifts.
    pub voice_temperature: f32,
    pub voice_top_k: u32,
    pub voice_top_p: f32,
    pub voice_repetition_penalty: f32,
    /// Silence (seconds) between spoken segments within narration. Engine
    /// default is 0.15; raise it (e.g. 0.35–0.6) for a storytelling pace.
    pub segment_pause: f32,
    /// Silence (seconds) at paragraph boundaries (usually > segment_pause).
    pub paragraph_pause: f32,

    pub workflow_version: u32,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            site_name: "Ruin".into(),
            voice: "Bình An".into(),
            emotion: "natural".into(),
            format: "mp3".into(),
            wpm: 145.0,
            cap_seconds: 90.0 * 60.0,
            overhead_seconds: 90.0,
            width: 1920,
            height: 1080,
            background_path: None,
            background_is_video: false,
            intro_music_path: None,
            bg_music_path: None,
            duck: DuckCfg::default(),
            intro_template: "Xin chào quý vị và các bạn. Đây là truyện {{novel.title}} của tác giả {{novel.author}}. Sau đây là {{chapter.range}}.".into(),
            outro_template: "Cảm ơn quý vị đã lắng nghe. Hẹn gặp lại trong tập tiếp theo trên kênh {{site.name}}.".into(),
            title_template: "{{novel.title}} | {{chapter.range}} | {{site.name}}".into(),
            description_template: "{{novel.title}} ({{novel.originalTitle}}) — {{novel.author}}\n{{chapter.range}}\n\nĐọc bởi {{site.name}}.".into(),
            tags_template: "truyện audio, {{novel.title}}, {{site.name}}, audiobook".into(),
            delay_before_intro: 0.8,
            delay_after_intro: 0.8,
            delay_after_content: 0.8,
            delay_after_outro: 1.2,
            voice_temperature: 0.5,
            voice_top_k: 25,
            voice_top_p: 0.9,
            voice_repetition_penalty: 1.3,
            segment_pause: 0.35,
            paragraph_pause: 0.7,
            workflow_version: 1,
        }
    }
}
