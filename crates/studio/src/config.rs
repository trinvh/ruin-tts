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
            workflow_version: 1,
        }
    }
}
