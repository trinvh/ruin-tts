//! Subtitle rendering as an ASS file burned by libass — the single source of
//! truth shared with the preview. libass handles word-wrapping (within margins),
//! the bundled Vietnamese font, outline and bottom-centre placement, so the
//! exported subtitle matches the CSS preview (which uses the same font + a
//! PlayRes-relative size).
//!
//! All sizes/margins are expressed against a 1920×1080 reference canvas
//! (`PlayResX/Y`); libass scales them to the real frame, exactly like the
//! preview scales `1cqh` to the stage. Keep this in sync with the preview
//! constants in `PreviewStage`/`subtitleStyle.ts`.

/// Reference canvas the sizes are authored against (libass scales to the frame).
pub const PLAY_W: i32 = 1920;
pub const PLAY_H: i32 = 1080;
/// Bundled font's family name (must match the embedded TTF's name table).
pub const FONT_NAME: &str = "Be Vietnam Pro SemiBold";
/// The bundled Vietnamese subtitle font (OFL), embedded so the export is
/// self-contained — written next to the ASS file and pointed at via `fontsdir`.
pub const FONT_TTF: &[u8] = include_bytes!("../../assets/BeVietnamPro-SemiBold.ttf");
/// `sub_size` (slider 18..52) → px at 1080 (so the default ~30 reads ~48px).
pub const SIZE_FACTOR: f64 = 1.6;
/// Subtitle text wraps within this fraction of the width; the rest is margin.
pub const MAX_WIDTH_FRAC: f64 = 0.86;
/// Distance of the subtitle baseline block from the bottom, as a fraction of H.
pub const MARGIN_V_FRAC: f64 = 0.075;

/// One subtitle line: timing + Vietnamese text (+ optional source for bilingual).
pub struct SubEvent<'a> {
    pub start_s: f64,
    pub end_s: f64,
    pub vi: &'a str,
    pub src: Option<&'a str>,
}

/// The persisted subtitle look (mirrors the preview).
pub struct SubStyle<'a> {
    /// Raw `sub_size` slider value (scaled by `SIZE_FACTOR` to px@1080).
    pub size: f64,
    /// `#RRGGBB`.
    pub color: &'a str,
    pub bilingual: bool,
    /// Draw a semi-transparent box behind the text (matches the preview box).
    pub bg: bool,
}

/// `#RRGGBB` → ASS `&HAABBGGRR` (opaque). Falls back to white on bad input.
pub fn ass_color(hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    if h.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        ) {
            return format!("&H00{b:02X}{g:02X}{r:02X}");
        }
    }
    "&H00FFFFFF".to_string()
}

/// Seconds → ASS timestamp `H:MM:SS.cc` (centiseconds).
pub fn ass_time(t: f64) -> String {
    let t = t.max(0.0);
    let cs = (t * 100.0).round() as i64;
    let (h, rem) = (cs / 360_000, cs % 360_000);
    let (m, rem) = (rem / 6_000, rem % 6_000);
    let (s, c) = (rem / 100, rem % 100);
    format!("{h}:{m:02}:{s:02}.{c:02}")
}

/// Escape subtitle text for an ASS Dialogue field: literal braces would start an
/// override block, and real newlines become hard breaks.
fn escape_ass(s: &str) -> String {
    s.replace('\\', "\\\u{200b}")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\r', "")
        .replace('\n', "\\N")
}

/// Build a full ASS document for the given events + style. Bottom-centred,
/// outlined, wrapped within `MAX_WIDTH_FRAC`. Returns the file contents.
pub fn build_ass(events: &[SubEvent], style: &SubStyle) -> String {
    let size = (style.size * SIZE_FACTOR).round().max(8.0);
    let color = ass_color(style.color);
    let margin_lr = ((PLAY_W as f64 * (1.0 - MAX_WIDTH_FRAC) / 2.0).round() as i32).max(0);
    let margin_v = (PLAY_H as f64 * MARGIN_V_FRAC).round() as i32;
    // A semi-transparent box (BorderStyle 4) when bg is on, else a thin text
    // outline (BorderStyle 1). No drop shadow either way — the old thick outline
    // + shadow read as too heavy. ASS alpha is inverted (00=opaque, FF=clear);
    // &H80000000 ≈ a 50% black box, matching the preview's rgba(0,0,0,.5).
    let (border_style, outline, back) = if style.bg {
        (4, 1.0_f64, "&H80000000")
    } else {
        (1, (size * 0.055).round().max(1.5), "&H00000000")
    };

    let mut out = String::new();
    out.push_str("[Script Info]\n");
    out.push_str("ScriptType: v4.00+\n");
    out.push_str("WrapStyle: 0\n");
    out.push_str("ScaledBorderAndShadow: yes\n");
    out.push_str(&format!("PlayResX: {PLAY_W}\nPlayResY: {PLAY_H}\n\n"));

    out.push_str("[V4+ Styles]\n");
    out.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    out.push_str(&format!(
        "Style: Default,{FONT_NAME},{size},{color},&H000000FF,&H00000000,{back},0,0,0,0,100,100,0,0,{border_style},{outline},0,2,{margin_lr},{margin_lr},{margin_v},1\n\n",
    ));

    out.push_str("[Events]\n");
    out.push_str(
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
    );
    for e in events {
        if e.vi.trim().is_empty() && e.src.map(|s| s.trim().is_empty()).unwrap_or(true) {
            continue;
        }
        // Bilingual: smaller source line above the Vietnamese line.
        let text = match (style.bilingual, e.src) {
            (true, Some(src)) if !src.trim().is_empty() => {
                format!(
                    "{{\\fs{}}}{}\\N{}",
                    (size * 0.72).round(),
                    escape_ass(src),
                    escape_ass(e.vi)
                )
            }
            _ => escape_ass(e.vi),
        };
        out.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            ass_time(e.start_s),
            ass_time(e.end_s),
            text,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_converts_rrggbb_to_ass_bgr() {
        assert_eq!(ass_color("#FFFFFF"), "&H00FFFFFF");
        assert_eq!(ass_color("#FFE082"), "&H0082E0FF"); // amber → BGR
        assert_eq!(ass_color("bad"), "&H00FFFFFF"); // fallback
    }

    #[test]
    fn time_formats_centiseconds() {
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(75.5), "0:01:15.50");
        assert_eq!(ass_time(3661.234), "1:01:01.23");
    }

    #[test]
    fn build_ass_has_font_resolution_and_wrapped_event() {
        let events = [SubEvent {
            start_s: 1.0,
            end_s: 3.0,
            vi: "Xin chào các bạn",
            src: None,
        }];
        let style = SubStyle {
            size: 30.0,
            color: "#FFFFFF",
            bilingual: false,
            bg: false,
        };
        let ass = build_ass(&events, &style);
        assert!(ass.contains("PlayResX: 1920"));
        assert!(ass.contains("PlayResY: 1080"));
        assert!(ass.contains(FONT_NAME));
        assert!(ass.contains("WrapStyle: 0")); // libass wraps long lines
        assert!(ass.contains("Fontsize") || ass.contains(",48,")); // 30*1.6
        assert!(ass.contains("Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Xin chào các bạn"));
    }

    #[test]
    fn build_ass_bilingual_stacks_source_above() {
        let events = [SubEvent {
            start_s: 0.0,
            end_s: 2.0,
            vi: "Việt",
            src: Some("源"),
        }];
        let style = SubStyle {
            size: 30.0,
            color: "#FFFFFF",
            bilingual: true,
            bg: false,
        };
        let ass = build_ass(&events, &style);
        assert!(ass.contains("源\\NViệt")); // source, hard break, vietnamese
    }

    #[test]
    fn build_ass_escapes_braces_and_newlines() {
        let events = [SubEvent {
            start_s: 0.0,
            end_s: 1.0,
            vi: "a{b}\nc",
            src: None,
        }];
        let style = SubStyle {
            size: 30.0,
            color: "#FFFFFF",
            bilingual: false,
            bg: false,
        };
        let ass = build_ass(&events, &style);
        assert!(ass.contains("a\\{b\\}\\Nc"));
    }
}
