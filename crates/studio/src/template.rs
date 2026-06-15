//! Template engine for operator-authored text (intro/outro/title/description/
//! tags). Handlebars with HTML-escaping disabled — output is plain Vietnamese
//! text/speech, never HTML.

use anyhow::Result;
use handlebars::Handlebars;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NovelVars {
    pub title: String,
    pub author: String,
    pub original_title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChapterVars {
    pub first: u32,
    pub last: u32,
    pub range: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoVars {
    pub index: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteVars {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateVars {
    pub novel: NovelVars,
    pub chapter: ChapterVars,
    pub video: VideoVars,
    pub site: SiteVars,
}

pub struct MakeVars {
    pub novel: NovelVars,
    pub first: u32,
    pub last: u32,
    pub chapter_title: String,
    pub video_index: u32,
    pub site_name: String,
}

/// "Chương 1–5", or "Chương 7" when first == last (en dash).
pub fn format_range(first: u32, last: u32) -> String {
    if first == last {
        format!("Chương {first}")
    } else {
        format!("Chương {first}–{last}")
    }
}

pub fn make_vars(input: MakeVars) -> TemplateVars {
    TemplateVars {
        novel: input.novel,
        chapter: ChapterVars {
            first: input.first,
            last: input.last,
            range: format_range(input.first, input.last),
            title: input.chapter_title,
        },
        video: VideoVars {
            index: input.video_index,
        },
        site: SiteVars {
            name: input.site_name,
        },
    }
}

/// Render a template string with the given variables (no HTML escaping).
pub fn render<T: Serialize>(template: &str, vars: &T) -> Result<String> {
    let mut hb = Handlebars::new();
    hb.register_escape_fn(handlebars::no_escape);
    Ok(hb.render_template(template, vars)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> TemplateVars {
        make_vars(MakeVars {
            novel: NovelVars {
                title: "Yêu Thần Ký".into(),
                author: "Phát Tiêu Đích Oa Ngưu".into(),
                original_title: "妖神记".into(),
            },
            first: 1,
            last: 5,
            chapter_title: "Khởi đầu".into(),
            video_index: 3,
            site_name: "Ruin".into(),
        })
    }

    #[test]
    fn range_formats() {
        assert_eq!(format_range(1, 5), "Chương 1–5");
        assert_eq!(format_range(7, 7), "Chương 7");
    }

    #[test]
    fn computes_range_in_vars() {
        assert_eq!(vars().chapter.range, "Chương 1–5");
    }

    #[test]
    fn renders_all_variables() {
        let tpl = "{{novel.title}} ({{novel.originalTitle}}) — {{novel.author}} | {{chapter.range}}: {{chapter.title}} | tập {{video.index}} | {{site.name}}";
        assert_eq!(
            render(tpl, &vars()).unwrap(),
            "Yêu Thần Ký (妖神记) — Phát Tiêu Đích Oa Ngưu | Chương 1–5: Khởi đầu | tập 3 | Ruin"
        );
    }

    #[test]
    fn unknown_vars_blank() {
        assert_eq!(render("x {{nope.nada}} y", &vars()).unwrap(), "x  y");
    }

    #[test]
    fn no_html_escaping() {
        let v = make_vars(MakeVars {
            novel: NovelVars {
                title: "A & B \"C\"".into(),
                author: String::new(),
                original_title: String::new(),
            },
            first: 1,
            last: 1,
            chapter_title: String::new(),
            video_index: 1,
            site_name: "Ruin".into(),
        });
        assert_eq!(render("{{novel.title}}", &v).unwrap(), "A & B \"C\"");
    }
}
