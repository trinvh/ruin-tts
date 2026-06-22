//! ffmpeg command construction for the audio/video pipeline. The argument
//! builders are pure and unit-tested; execution shells out to ffmpeg/ffprobe.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// The ffmpeg binary to invoke: `FFMPEG_PATH` if it points at an existing file
/// (e.g. one the app downloaded during onboarding), else `ffmpeg` from PATH.
pub fn ffmpeg_bin() -> String {
    bin_from_env("FFMPEG_PATH", "ffmpeg")
}
pub fn ffprobe_bin() -> String {
    bin_from_env("FFPROBE_PATH", "ffprobe")
}
fn bin_from_env(var: &str, fallback: &str) -> String {
    match std::env::var(var) {
        Ok(p) if !p.is_empty() && Path::new(&p).exists() => p,
        _ => fallback.to_string(),
    }
}

/// Settings for ducking the background music bed under the voice.
#[derive(Debug, Clone, Copy)]
pub struct DuckSettings {
    /// Background music gain before ducking (0..1).
    pub music_volume: f64,
    pub threshold: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
}

impl Default for DuckSettings {
    fn default() -> Self {
        Self {
            music_volume: 0.25,
            threshold: 0.03,
            ratio: 8.0,
            attack: 20.0,
            release: 300.0,
        }
    }
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Concatenate narration segments into one track (concat filter, re-encoded).
pub fn concat_audio_args(inputs: &[&Path], out: &Path) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    for i in inputs {
        args.push("-i".into());
        args.push(s(i));
    }
    let streams: String = (0..inputs.len()).map(|i| format!("[{i}:a]")).collect();
    let fc = format!("{streams}concat=n={}:v=0:a=1[out]", inputs.len());
    args.extend([
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "[out]".into(),
        s(out),
    ]);
    args
}

/// Mix voice over a looped, side-chain-ducked background music bed. Output
/// length follows the voice (`duration=first`).
pub fn duck_mix_args(voice: &Path, music: &Path, out: &Path, d: DuckSettings) -> Vec<String> {
    let fc = format!(
        "[1:a]volume={mv}[m];[m][0:a]sidechaincompress=threshold={th}:ratio={ra}:attack={at}:release={re}[mduck];[0:a][mduck]amix=inputs=2:duration=first:dropout_transition=0[out]",
        mv = d.music_volume, th = d.threshold, ra = d.ratio, at = d.attack, re = d.release,
    );
    vec![
        "-y".into(),
        "-i".into(),
        s(voice),
        "-stream_loop".into(),
        "-1".into(),
        "-i".into(),
        s(music),
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "[out]".into(),
        s(out),
    ]
}

/// A segment of an assembled audio track: a real clip or a gap of silence.
pub enum AudioPart {
    File(PathBuf),
    Silence(f64),
}

/// Assemble parts in order into one track, inserting real silence for gaps
/// (e.g. `<delay> intro <delay> content <delay> outro <delay>`). Silence is
/// generated with `anullsrc` so it concatenates cleanly with the clips.
pub fn assemble_args(parts: &[AudioPart], out: &Path) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    for p in parts {
        match p {
            AudioPart::File(f) => {
                args.push("-i".into());
                args.push(s(f));
            }
            AudioPart::Silence(d) => {
                args.extend([
                    "-f".into(),
                    "lavfi".into(),
                    "-t".into(),
                    format!("{:.3}", d.max(0.0)),
                    "-i".into(),
                    "anullsrc=r=48000:cl=mono".into(),
                ]);
            }
        }
    }
    let streams: String = (0..parts.len()).map(|i| format!("[{i}:a]")).collect();
    let fc = format!("{streams}concat=n={}:v=0:a=1[out]", parts.len());
    args.extend([
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "[out]".into(),
        s(out),
    ]);
    args
}

/// Mix narration clips onto the source timeline: each clip is delayed to its
/// absolute start (`adelay`) and summed (`amix`, no level normalization) over a
/// silent base spanning `total_secs`. Clips land at exactly their start time and
/// overlapping speech overlaps in the output (matching the original video)
/// instead of being serialized + drifting. `clips` = (path, start_seconds).
pub fn mix_at_times_args(clips: &[(PathBuf, f64)], total_secs: f64, out: &Path) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    for (p, _) in clips {
        args.push("-i".into());
        args.push(s(p));
    }
    // Silent base spanning the whole timeline (last input) so the track always
    // covers the full duration even if no clip reaches the end.
    let base_idx = clips.len();
    args.extend([
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        format!("{:.3}", total_secs.max(0.0)),
        "-i".into(),
        "anullsrc=r=48000:cl=mono".into(),
    ]);
    let mut fc = String::new();
    for (i, (_, start)) in clips.iter().enumerate() {
        let ms = ((*start).max(0.0) * 1000.0).round() as i64;
        fc.push_str(&format!("[{i}]adelay={ms}[c{i}];"));
    }
    fc.push_str(&format!("[{base_idx}]"));
    for i in 0..clips.len() {
        fc.push_str(&format!("[c{i}]"));
    }
    fc.push_str(&format!(
        "amix=inputs={}:normalize=0:duration=longest[out]",
        clips.len() + 1
    ));
    args.extend([
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "[out]".into(),
        s(out),
    ]);
    args
}

/// Silence padding around the spoken parts of one chunk's voice track.
#[derive(Debug, Clone, Copy)]
pub struct VoiceDelays {
    pub before_intro: f64,
    pub after_intro: f64,
    pub after_content: f64,
    pub after_outro: f64,
}

fn push_silence(parts: &mut Vec<AudioPart>, secs: f64) {
    if secs > 0.0 {
        parts.push(AudioPart::Silence(secs));
    }
}

/// Build the ordered parts for one chunk's voice track:
/// `<before> intro <after_intro> content… <after_content> outro <after_outro>`.
/// Missing intro/outro (and any zero delay) are skipped so the result always
/// concatenates cleanly.
pub fn voice_sequence(
    intro: Option<&Path>,
    content: &[&Path],
    outro: Option<&Path>,
    d: &VoiceDelays,
) -> Vec<AudioPart> {
    let mut parts: Vec<AudioPart> = Vec::new();
    if let Some(i) = intro {
        push_silence(&mut parts, d.before_intro);
        parts.push(AudioPart::File(i.to_path_buf()));
        push_silence(&mut parts, d.after_intro);
    }
    for c in content {
        parts.push(AudioPart::File(c.to_path_buf()));
    }
    if let Some(o) = outro {
        push_silence(&mut parts, d.after_content);
        parts.push(AudioPart::File(o.to_path_buf()));
        push_silence(&mut parts, d.after_outro);
    }
    parts
}

/// Concatenate an intro music clip before the main (narration) track.
pub fn prepend_intro_music_args(intro: &Path, main: &Path, out: &Path) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        s(intro),
        "-i".into(),
        s(main),
        "-filter_complex".into(),
        "[0:a][1:a]concat=n=2:v=0:a=1[out]".into(),
        "-map".into(),
        "[out]".into(),
        s(out),
    ]
}

/// Compose a video from an audio track over a looping background image/video.
pub fn compose_video_args(
    audio: &Path,
    background: &Path,
    out: &Path,
    background_is_video: bool,
    width: u32,
    height: u32,
) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    if background_is_video {
        args.extend([
            "-stream_loop".into(),
            "-1".into(),
            "-i".into(),
            s(background),
        ]);
    } else {
        args.extend(["-loop".into(), "1".into(), "-i".into(), s(background)]);
    }
    args.extend(["-i".into(), s(audio)]);
    let vf = format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1");
    args.extend([
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        vf,
        "-c:v".into(),
        "libx264".into(),
        "-tune".into(),
        "stillimage".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-shortest".into(),
        s(out),
    ]);
    args
}

/// Extract a mono 16 kHz WAV from a video/audio file (the format media-ai +
/// most ASR/diarization models expect).
pub fn extract_audio_args(input: &Path, out: &Path) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        s(input),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        s(out),
    ]
}

/// Change a clip's tempo (without pitch shift) to fit a time slot. `atempo`
/// only accepts 0.5–2.0, so larger factors are split into a chained filter.
pub fn atempo_args(input: &Path, factor: f64, out: &Path) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        s(input),
        "-filter:a".into(),
        atempo_chain(factor),
        s(out),
    ]
}

/// Build an `atempo` filter chain for an arbitrary positive factor by composing
/// stages each within ffmpeg's [0.5, 2.0] bound (e.g. 2.5 → atempo=2.0,atempo=1.25).
pub fn atempo_chain(factor: f64) -> String {
    let mut remaining = factor.max(0.25);
    let mut stages: Vec<String> = Vec::new();
    while remaining > 2.0 {
        stages.push("atempo=2.0".into());
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        stages.push("atempo=0.5".into());
        remaining /= 0.5;
    }
    stages.push(format!("atempo={remaining:.4}"));
    stages.join(",")
}

/// Mix a dubbed voice track over the original video, lowering the original audio
/// to `original_volume` (0..1). Output keeps the original video stream and a new
/// mixed stereo audio track. `amix` with `duration=longest` keeps the full video.
pub fn mux_dub_args(video: &Path, voice: &Path, out: &Path, original_volume: f64) -> Vec<String> {
    let fc = format!(
        "[0:a]volume={ov:.3}[orig];[orig][1:a]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[a]",
        ov = original_volume.clamp(0.0, 1.0),
    );
    vec![
        "-y".into(),
        "-i".into(),
        s(video),
        "-i".into(),
        s(voice),
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "[a]".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        s(out),
    ]
}

/// Audio-only export: mix the original audio (at `original_volume`) with the
/// Vietnamese dub (at `vn_volume`) into a single AAC file — used when the video
/// track is deleted in the editor ("chỉ có tiếng").
pub fn export_audio_args(
    video: &Path,
    voice: &Path,
    out: &Path,
    original_volume: f64,
    vn_volume: f64,
    lead_in: f64,
) -> Vec<String> {
    let delay = if lead_in > 0.0 {
        format!(
            ";[amx]adelay={ms}:all=1[a]",
            ms = (lead_in * 1000.0).round() as i64
        )
    } else {
        String::new()
    };
    let mix_out = if lead_in > 0.0 { "[amx]" } else { "[a]" };
    let fc = format!(
        "[0:a]volume={ov:.3}[orig];[1:a]volume={vv:.3}[vn];[orig][vn]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0{mix_out}{delay}",
        ov = original_volume.clamp(0.0, 1.0),
        vv = vn_volume.clamp(0.0, 1.0),
    );
    vec![
        "-y".into(),
        "-i".into(),
        s(video),
        "-i".into(),
        s(voice),
        "-filter_complex".into(),
        fc,
        "-map".into(),
        "[a]".into(),
        "-vn".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        s(out),
    ]
}

/// One subtitle cue: start/end seconds + text, plus an optional line rendered
/// above it (used for bilingual source-over-Vietnamese subtitles).
pub struct Cue<'a> {
    pub start: f64,
    pub end: f64,
    pub text: &'a str,
    pub top: Option<&'a str>,
}

fn srt_time(t: f64) -> String {
    let t = t.max(0.0);
    let ms = (t * 1000.0).round() as u64;
    let (h, m, s, milli) = (
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000,
    );
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

/// Build an SRT document from cues (empty-text cues skipped). When a cue carries
/// a non-empty `top` line it is emitted above the main text (bilingual: source
/// over Vietnamese).
pub fn build_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    let mut n = 1;
    for c in cues {
        if c.text.trim().is_empty() {
            continue;
        }
        let body = match c.top {
            Some(t) if !t.trim().is_empty() => format!("{}\n{}", t.trim(), c.text.trim()),
            _ => c.text.trim().to_string(),
        };
        out.push_str(&format!(
            "{n}\n{} --> {}\n{body}\n\n",
            srt_time(c.start),
            srt_time(c.end.max(c.start + 0.1)),
        ));
        n += 1;
    }
    out
}

/// Convert a `#RRGGBB` hex colour to an ASS `&HBBGGRR&` literal (libass byte
/// order is B,G,R with a leading alpha of `00` = fully opaque). Falls back to
/// white (`&H00FFFFFF&`) when the input isn't a clean 6-digit hex.
fn ass_color(hex: &str) -> String {
    let t = hex.trim().trim_start_matches('#');
    if t.len() == 6 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        let (r, g, b) = (&t[0..2], &t[2..4], &t[4..6]);
        format!(
            "&H00{}{}{}&",
            b.to_uppercase(),
            g.to_uppercase(),
            r.to_uppercase()
        )
    } else {
        "&H00FFFFFF&".to_string()
    }
}

/// Escape a path for use inside the ffmpeg `subtitles=` filter value.
fn escape_filter_path(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

/// Options for the final dub export.
pub struct ExportOpts<'a> {
    pub original_volume: f64,
    /// Volume of the Vietnamese dub track (0..1).
    pub vn_volume: f64,
    /// Burn this SRT into the video via libass (re-encodes). Requires the
    /// `subtitles` filter; only set when [`has_filter`] confirms it.
    pub subtitles_burn: Option<&'a Path>,
    /// Embed this SRT as a selectable soft track (mov_text) — the fallback when
    /// ffmpeg lacks libass, so subtitles still ship even if not hard-coded.
    pub subtitles_soft: Option<&'a Path>,
    /// Subtitle vertical margin from the bottom, in pixels (libass `MarginV`).
    pub sub_margin_v: Option<u32>,
    /// Burned-subtitle font size in pixels (libass `FontSize`).
    pub sub_size: Option<f64>,
    /// Burned-subtitle colour as a `#RRGGBB` hex string (libass `PrimaryColour`).
    pub sub_color: Option<&'a str>,
    /// Blur a rectangle `(x, y, w, h)` (fractions of the frame) to hide original
    /// hard-coded subtitles (re-encodes).
    pub blur: Option<(f64, f64, f64, f64)>,
    /// Frame size in pixels — when present, the blur edges are feathered (needs
    /// pixel coords for the soft alpha mask); without it, a hard-edged fallback.
    pub frame: Option<(u32, u32)>,
    /// Image/banner overlays burned over the video for their time range.
    pub overlays: Vec<OverlayArg<'a>>,
    /// Lead-in seconds: pad the video with black at the front and delay the audio
    /// by this amount (subtitle/overlay times are pre-shifted by the caller).
    pub lead_in: f64,
}

/// One image overlay for the export: a file placed at a fractional position/size
/// over `[start, end]` (end ≤ start ⇒ always shown).
pub struct OverlayArg<'a> {
    pub path: &'a Path,
    pub start: f64,
    pub end: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub opacity: f64,
}

/// Build the blur filterchain producing `[vb]`. With known frame dimensions the
/// blurred patch gets a feathered (soft) alpha so its edges fade; otherwise a
/// simple hard-edged crop+overlay (fractional, resolution-independent).
fn blur_filterchain(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    frame: Option<(u32, u32)>,
    src: &str,
) -> String {
    if let Some((vw, vh)) = frame {
        let cw = ((vw as f64 * w).round() as i64).max(2);
        let ch = ((vh as f64 * h).round() as i64).max(2);
        let cx = (vw as f64 * x).round() as i64;
        let cy = (vh as f64 * y).round() as i64;
        // Feather width: a fraction of the shorter side, bounded so the inner
        // white box stays positive.
        let fp = ((cw.min(ch) as f64 * 0.18).round() as i64)
            .clamp(2, 48)
            .min(cw / 2 - 1)
            .min(ch / 2 - 1)
            .max(1);
        let (iw, ih) = (cw - 2 * fp, ch - 2 * fp);
        format!(
            "{src}split=2[bg][fg];\
             [fg]crop={cw}:{ch}:{cx}:{cy},gblur=sigma=18,format=yuva420p[bl];\
             color=black:s={cw}x{ch},drawbox=x={fp}:y={fp}:w={iw}:h={ih}:color=white:t=fill,gblur=sigma={fps:.1}[mask];\
             [bl][mask]alphamerge[bm];\
             [bg][bm]overlay={cx}:{cy}[vb]",
            fps = fp as f64,
        )
    } else {
        format!(
            "{src}split=2[bg][fg];[fg]crop=iw*{w:.4}:ih*{h:.4}:iw*{x:.4}:ih*{y:.4},gblur=sigma=18[bl];[bg][bl]overlay=W*{x:.4}:H*{y:.4}[vb]"
        )
    }
}

/// Check whether a given ffmpeg filter (e.g. `subtitles`) is available in the
/// installed build — used to decide burn-in vs. soft-subtitle fallback.
pub async fn has_filter(name: &str) -> bool {
    match tokio::process::Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-filters"])
        .output()
        .await
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some(name)),
        Err(_) => false,
    }
}

/// Mux the Vietnamese track over the original video, optionally blurring a region
/// and/or adding subtitles (burned via libass, or embedded as a soft track). The
/// video stream is copied unless a video filter (blur/burn) forces a re-encode.
pub fn export_video_args(video: &Path, voice: &Path, out: &Path, opts: &ExportOpts) -> Vec<String> {
    let lead = opts.lead_in.max(0.0);
    // Mix original + VN; when there's a lead-in, delay the whole mix so it lines
    // up with the black-padded video.
    let audio_fc = if lead > 0.0 {
        format!(
            "[0:a]volume={ov:.3}[orig];[1:a]volume={vv:.3}[vn];[orig][vn]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[amx];[amx]adelay={ms}:all=1[a]",
            ov = opts.original_volume.clamp(0.0, 1.0),
            vv = opts.vn_volume.clamp(0.0, 1.0),
            ms = (lead * 1000.0).round() as i64,
        )
    } else {
        format!(
            "[0:a]volume={ov:.3}[orig];[1:a]volume={vv:.3}[vn];[orig][vn]amix=inputs=2:duration=longest:dropout_transition=0:normalize=0[a]",
            ov = opts.original_volume.clamp(0.0, 1.0),
            vv = opts.vn_volume.clamp(0.0, 1.0),
        )
    };

    // Video filter chain (only when a lead-in, blur or burned subtitles apply).
    let mut vchain: Vec<String> = Vec::new();
    let mut vlabel = "0:v:0".to_string(); // stream spec used when not filtered (copy)
                                          // Lead-in must come first so blur/subtitles/overlays sit on the padded timeline.
    if lead > 0.0 {
        vchain.push(format!(
            "[0:v]tpad=start_duration={lead:.3}:start_mode=add:color=black[vlead]"
        ));
        vlabel = "[vlead]".to_string();
    }
    if let Some((x, y, w, h)) = opts.blur {
        let x = x.clamp(0.0, 0.99);
        let y = y.clamp(0.0, 0.99);
        let w = w.clamp(0.02, 1.0 - x);
        let h = h.clamp(0.02, 1.0 - y);
        let base = if vchain.is_empty() {
            "[0:v]".to_string()
        } else {
            vlabel.clone()
        };
        // `split` (use the source twice) + `gblur` (sigma-based, unlike boxblur
        // whose radius must not exceed the region). Edges are feathered via a
        // soft alpha mask when the frame size is known. NOTE: `overlay`'s x/y use
        // `W`/`H`; `iw`/`ih` are undefined there (only in `crop`) → EINVAL (-22).
        vchain.push(blur_filterchain(x, y, w, h, opts.frame, &base));
        vlabel = "[vb]".to_string();
    }
    if let Some(sub) = opts.subtitles_burn {
        let base = if vchain.is_empty() {
            "[0:v]".to_string()
        } else {
            vlabel.clone()
        };
        // libass `force_style` properties (MarginV, FontSize, PrimaryColour).
        // Inside a filtergraph a bare comma is a filter separator, so the entries
        // are joined with an escaped `\,` which ffmpeg passes through to libass.
        let mut props: Vec<String> = Vec::new();
        if let Some(m) = opts.sub_margin_v {
            props.push(format!("MarginV={m}"));
        }
        if let Some(sz) = opts.sub_size {
            props.push(format!("FontSize={}", sz.round() as i64));
        }
        if let Some(c) = opts.sub_color {
            props.push(format!("PrimaryColour={}", ass_color(c)));
        }
        let style = if props.is_empty() {
            String::new()
        } else {
            format!(":force_style='{}'", props.join("\\,"))
        };
        vchain.push(format!(
            "{base}subtitles='{}'{style}[vs]",
            escape_filter_path(sub)
        ));
        vlabel = "[vs]".to_string();
    }

    // Image/banner overlays, each on its own input, scaled to a fraction of the
    // frame width, faded by `opacity`, and shown only over its time range. Inputs
    // come after the optional soft-subtitle input (index 2).
    let overlay_base_idx = 2 + usize::from(opts.subtitles_soft.is_some());
    let mut overlay_paths: Vec<String> = Vec::new();
    for (i, ov) in opts.overlays.iter().enumerate() {
        let inp = overlay_base_idx + i;
        overlay_paths.push(s(ov.path));
        let base = if vchain.is_empty() {
            "[0:v]".to_string()
        } else {
            vlabel.clone()
        };
        let scale = match opts.frame {
            Some((vw, _)) => format!("scale={}:-1", ((vw as f64 * ov.w).round() as i64).max(2)),
            None => "scale=iw:-1".to_string(),
        };
        let en = if ov.end > ov.start {
            format!(
                ":enable='between(t,{:.3},{:.3})'",
                ov.start.max(0.0),
                ov.end
            )
        } else {
            String::new()
        };
        vchain.push(format!(
            "[{inp}:v]{scale},format=rgba,colorchannelmixer=aa={op:.3}[ovi{i}];{base}[ovi{i}]overlay=W*{x:.4}:H*{y:.4}{en}[vov{i}]",
            op = ov.opacity.clamp(0.0, 1.0),
            x = ov.x.clamp(0.0, 1.0),
            y = ov.y.clamp(0.0, 1.0),
        ));
        vlabel = format!("[vov{i}]");
    }

    let filtered = !vchain.is_empty();

    let mut args = vec!["-y".into(), "-i".into(), s(video), "-i".into(), s(voice)];
    if let Some(soft) = opts.subtitles_soft {
        args.push("-i".into());
        args.push(s(soft)); // input index 2
    }
    for p in &overlay_paths {
        args.push("-i".into());
        args.push(p.clone());
    }

    let fc = if filtered {
        format!("{audio_fc};{}", vchain.join(";"))
    } else {
        audio_fc
    };
    args.extend(["-filter_complex".into(), fc]);
    args.extend(["-map".into(), vlabel, "-map".into(), "[a]".into()]);
    if opts.subtitles_soft.is_some() {
        args.extend(["-map".into(), "2:0".into()]);
    }
    if filtered {
        args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ]);
    } else {
        args.extend(["-c:v".into(), "copy".into()]);
    }
    args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    if opts.subtitles_soft.is_some() {
        args.extend(["-c:s".into(), "mov_text".into()]);
    }
    args.push(s(out));
    args
}

/// The kind of a timeline clip fed to [`compose_export_args`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Video,
    Audio,
    Image,
    Text,
}

/// One clip on the general compositing timeline. Geometry (`x`/`y`/`w`/`opacity`)
/// is fractional (resolution-independent); `start`/`dur` place it on the timeline,
/// `in_s` trims the in-point inside the source media. `text` is only used for
/// [`ClipKind::Text`].
pub struct ClipArg<'a> {
    pub kind: ClipKind,
    pub source: Option<&'a Path>,
    pub start: f64,
    pub dur: f64,
    pub in_s: f64,
    pub volume: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub opacity: f64,
    pub text: Option<&'a str>,
}

/// Escape a string for use inside a `drawtext=text='…'` value. ffmpeg's drawtext
/// reads the value after the filtergraph tokenizer, so backslashes, colons, and
/// single quotes must be escaped (and `%`, which drawtext expands).
fn escape_drawtext(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            ':' => out.push_str("\\:"),
            '%' => out.push_str("\\%"),
            // Newlines would break the single-line drawtext value → use a space.
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Build a generalized compositing-export command from timeline `clips` over a
/// black canvas of `frame` size spanning `total_s` seconds. Video/image clips are
/// composited in input order (callers sort by track so higher tracks land on top);
/// audio clips are delayed to their start and mixed; text clips are drawn over the
/// running video via `drawtext`. The output is H.264 + AAC, bounded to `total_s`.
///
/// Input indexing: the black base canvas is the lavfi input 0; then every clip
/// that needs a file input (video/image/audio) is added in iteration order and
/// tracks its own `[N:…]` index. Text clips consume no input.
pub fn compose_export_args(
    clips: &[ClipArg],
    total_s: f64,
    frame: (u32, u32),
    out: &Path,
    text_ok: bool,
) -> Vec<String> {
    let (fw, fh) = frame;
    let total = total_s.max(0.1);

    // Input 0: the black base canvas (lavfi). Real file inputs follow.
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        format!("{total:.3}"),
        "-i".into(),
        format!("color=c=black:s={fw}x{fh}:r=30:d={total:.3}"),
    ];

    // Assign a real ffmpeg input index to each clip needing a file. Text clips get
    // None. The base canvas is index 0, so file inputs start at 1.
    let mut input_idx: Vec<Option<usize>> = Vec::with_capacity(clips.len());
    let mut next_idx = 1usize;
    for c in clips {
        match c.kind {
            ClipKind::Video | ClipKind::Image | ClipKind::Audio => {
                if let Some(src) = c.source {
                    args.push("-i".into());
                    args.push(s(src));
                    input_idx.push(Some(next_idx));
                    next_idx += 1;
                } else {
                    input_idx.push(None);
                }
            }
            ClipKind::Text => input_idx.push(None),
        }
    }

    let mut chains: Vec<String> = Vec::new();
    // The running base video label, starting at the lavfi canvas.
    let mut base = "0:v".to_string();
    let mut audio_labels: Vec<String> = Vec::new();

    for (i, c) in clips.iter().enumerate() {
        let start = c.start.max(0.0);
        let end = start + c.dur.max(0.0);
        let enable = format!("enable='between(t,{start:.3},{end:.3})'");
        match c.kind {
            ClipKind::Video => {
                let Some(idx) = input_idx[i] else { continue };
                let sw = ((fw as f64 * c.w).round() as i64).max(2);
                chains.push(format!(
                    "[{idx}:v]trim=start={in_s:.3}:duration={dur:.3},setpts=PTS-STARTPTS,scale={sw}:-2[v{i}]",
                    in_s = c.in_s.max(0.0),
                    dur = c.dur.max(0.0),
                ));
                chains.push(format!(
                    "[{base}][v{i}]overlay=W*{x:.4}:H*{y:.4}:{enable}[vc{i}]",
                    x = c.x,
                    y = c.y,
                ));
                base = format!("vc{i}");
            }
            ClipKind::Image => {
                let Some(idx) = input_idx[i] else { continue };
                let sw = ((fw as f64 * c.w).round() as i64).max(2);
                chains.push(format!(
                    "[{idx}:v]scale={sw}:-1,format=rgba,colorchannelmixer=aa={op:.3}[im{i}]",
                    op = c.opacity.clamp(0.0, 1.0),
                ));
                chains.push(format!(
                    "[{base}][im{i}]overlay=W*{x:.4}:H*{y:.4}:{enable}[vc{i}]",
                    x = c.x,
                    y = c.y,
                ));
                base = format!("vc{i}");
            }
            ClipKind::Text => {
                // `drawtext` needs libfreetype; when absent we skip text clips so
                // the rest of the export still renders.
                if !text_ok {
                    continue;
                }
                let Some(text) = c.text else { continue };
                if text.trim().is_empty() {
                    continue;
                }
                // Default font size scaled from the frame height (~4.5% of height).
                let sz = ((fh as f64 * 0.045).round() as i64).max(12);
                chains.push(format!(
                    "[{base}]drawtext=text='{txt}':x=(w-text_w)/2:y=h*0.85:fontsize={sz}:fontcolor=white:borderw=2:{enable}[vc{i}]",
                    txt = escape_drawtext(text),
                ));
                base = format!("vc{i}");
            }
            ClipKind::Audio => {
                let Some(idx) = input_idx[i] else { continue };
                let ms = (start * 1000.0).round() as i64;
                chains.push(format!(
                    "[{idx}:a]atrim=start={in_s:.3}:duration={dur:.3},asetpts=PTS-STARTPTS,volume={vol:.3},adelay={ms}:all=1[a{i}]",
                    in_s = c.in_s.max(0.0),
                    dur = c.dur.max(0.0),
                    vol = c.volume.max(0.0),
                ));
                audio_labels.push(format!("a{i}"));
            }
        }
    }

    // Audio: mix all audio clips, or generate silence so the output always has an
    // audio track.
    let audio_out = if audio_labels.is_empty() {
        // Append an anullsrc lavfi input for the silent track. Its input index is
        // `next_idx` (the file inputs occupied 1..next_idx).
        args.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            format!("{total:.3}"),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]);
        chains.push(format!("[{next_idx}:a]anull[a]"));
        "[a]".to_string()
    } else {
        let joined: String = audio_labels.iter().map(|l| format!("[{l}]")).collect();
        chains.push(format!(
            "{joined}amix=inputs={n}:normalize=0:duration=longest[a]",
            n = audio_labels.len(),
        ));
        "[a]".to_string()
    };

    // If no video/image/text filter ran, `base` is still the raw input stream
    // "0:v" (a stream specifier, mapped without brackets); otherwise it's a
    // filtergraph label like "vc3" (mapped with brackets).
    let final_video = if base == "0:v" {
        "0:v".to_string()
    } else {
        format!("[{base}]")
    };
    let fc = chains.join(";");
    args.extend([
        "-filter_complex".into(),
        fc,
        "-map".into(),
        final_video,
        "-map".into(),
        audio_out,
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-t".into(),
        format!("{total:.3}"),
        s(out),
    ]);
    args
}

pub fn ffprobe_duration_args(path: &Path) -> Vec<String> {
    vec![
        "-v".into(),
        "error".into(),
        "-show_entries".into(),
        "format=duration".into(),
        "-of".into(),
        "default=noprint_wrappers=1:nokey=1".into(),
        s(path),
    ]
}

/// Parse the duration (seconds) from `ffprobe` stdout.
pub fn parse_duration(stdout: &str) -> Result<f64> {
    stdout
        .trim()
        .parse::<f64>()
        .with_context(|| format!("parse ffprobe duration {stdout:?}"))
}

/// Run ffmpeg with the given args (requires `ffmpeg` on PATH). Captures stderr
/// so a failure (bad input, or a broken ffmpeg/dyld install) surfaces in the
/// run's error instead of only the console.
pub async fn run_ffmpeg(args: &[String]) -> Result<()> {
    let out = tokio::process::Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-nostats", "-loglevel", "error"])
        .args(args)
        .kill_on_drop(true) // a cancelled run drops this future → kill ffmpeg
        .output()
        .await
        .context("spawn ffmpeg (is it installed?)")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: String = {
            let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
            lines[lines.len().saturating_sub(8)..].join("\n")
        };
        let detail = if tail.trim().is_empty() {
            "(không có stderr — kiểm tra cài đặt ffmpeg, vd: `brew reinstall ffmpeg`)".to_string()
        } else {
            tail
        };
        return Err(anyhow!("ffmpeg lỗi ({}):\n{}", out.status, detail));
    }
    Ok(())
}

/// Probe a media file's key info (duration, size, video/audio codec, resolution,
/// fps) via `ffprobe`, returned as a trimmed JSON object for the UI.
pub async fn probe_media_info(path: &Path) -> Result<serde_json::Value> {
    let out = tokio::process::Command::new(ffprobe_bin())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(s(path))
        .output()
        .await
        .context("spawn ffprobe")?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).context("parse ffprobe json")?;
    let fmt = v.get("format");
    let streams = v.get("streams").and_then(|s| s.as_array());
    let by_type = |t: &str| {
        streams.and_then(|s| {
            s.iter()
                .find(|st| st.get("codec_type").and_then(|c| c.as_str()) == Some(t))
        })
    };
    let video = by_type("video");
    let audio = by_type("audio");
    let fps = video
        .and_then(|st| st.get("r_frame_rate"))
        .and_then(|r| r.as_str())
        .and_then(|r| {
            let (n, d) = r.split_once('/')?;
            let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
            if d > 0.0 {
                Some((n / d * 100.0).round() / 100.0)
            } else {
                None
            }
        });
    let num = |o: Option<&serde_json::Value>, k: &str| {
        o.and_then(|x| x.get(k))
            .and_then(|x| x.as_str())
            .and_then(|x| x.parse::<f64>().ok())
    };
    Ok(serde_json::json!({
        "duration": num(fmt, "duration"),
        "size": num(fmt, "size"),
        "format_name": fmt.and_then(|f| f.get("format_name")).and_then(|x| x.as_str()),
        "video": video.map(|st| serde_json::json!({
            "codec": st.get("codec_name").and_then(|x| x.as_str()),
            "profile": st.get("profile").and_then(|x| x.as_str()),
            "width": st.get("width").and_then(|x| x.as_u64()),
            "height": st.get("height").and_then(|x| x.as_u64()),
            "pix_fmt": st.get("pix_fmt").and_then(|x| x.as_str()),
            "fps": fps,
            "bit_rate": st.get("bit_rate").and_then(|x| x.as_str()),
        })),
        "audio": audio.map(|st| serde_json::json!({
            "codec": st.get("codec_name").and_then(|x| x.as_str()),
            "channels": st.get("channels").and_then(|x| x.as_u64()),
            "sample_rate": st.get("sample_rate").and_then(|x| x.as_str()),
        })),
    }))
}

/// Probe a video's pixel size `(width, height)` (first video stream).
pub async fn probe_video_dimensions(path: &Path) -> Result<(u32, u32)> {
    let out = tokio::process::Command::new(ffprobe_bin())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg(s(path))
        .output()
        .await
        .context("spawn ffprobe")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (w, h) = text
        .trim()
        .split_once('x')
        .ok_or_else(|| anyhow!("parse ffprobe size {text:?}"))?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}

/// Probe a video's pixel height (first video stream), for subtitle positioning.
pub async fn probe_video_height(path: &Path) -> Result<u32> {
    let out = tokio::process::Command::new(ffprobe_bin())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=height",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(s(path))
        .output()
        .await
        .context("spawn ffprobe")?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u32>()
        .with_context(|| "parse ffprobe height")
}

/// Probe a media file's duration in seconds (requires `ffprobe` on PATH).
pub async fn probe_duration(path: &Path) -> Result<f64> {
    let out = tokio::process::Command::new(ffprobe_bin())
        .args(ffprobe_duration_args(path))
        .output()
        .await
        .context("spawn ffprobe (is it installed?)")?;
    parse_duration(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn mix_at_times_places_clips_absolutely_and_mixes() {
        let clips = vec![(p("a.wav"), 0.0), (p("b.wav"), 2.5)];
        let a = mix_at_times_args(&clips, 9.0, &p("vn.wav"));
        let j = a.join(" ");
        // each clip delayed to its absolute start (ms), then summed
        assert!(j.contains("[0]adelay=0[c0];"));
        assert!(j.contains("[1]adelay=2500[c1];"));
        // base (index 2 = clips.len()) + 2 clips → amix of 3, no normalization
        assert!(j.contains("[2][c0][c1]amix=inputs=3:normalize=0:duration=longest[out]"));
        // a silent base spanning the timeline is the last input
        assert!(j.contains("anullsrc=r=48000:cl=mono"));
        assert!(j.contains("-t 9.000"));
    }

    #[test]
    fn mix_at_times_single_clip_still_mixes_over_base() {
        let a = mix_at_times_args(&[(p("only.wav"), 1.0)], 5.0, &p("vn.wav"));
        let j = a.join(" ");
        assert!(j.contains("[0]adelay=1000[c0];"));
        assert!(j.contains("[1][c0]amix=inputs=2:normalize=0:duration=longest[out]"));
    }

    #[test]
    fn duck_mix_loops_music_and_sidechains() {
        let a = duck_mix_args(
            &p("voice.wav"),
            &p("bg.mp3"),
            &p("out.wav"),
            DuckSettings::default(),
        );
        let joined = a.join(" ");
        assert!(joined.contains("sidechaincompress"));
        assert!(joined.contains("amix=inputs=2:duration=first"));
        // music input must be looped: ... -stream_loop -1 -i bg.mp3
        let i = a.iter().position(|x| x == "bg.mp3").unwrap();
        assert_eq!(a[i - 1], "-i");
        assert_eq!(a[i - 2], "-1");
        assert_eq!(a[i - 3], "-stream_loop");
        assert_eq!(a.last().unwrap(), "out.wav");
    }

    #[test]
    fn intro_music_prepends_via_concat() {
        let a = prepend_intro_music_args(&p("intro.mp3"), &p("main.wav"), &p("o.wav"));
        assert!(a.join(" ").contains("concat=n=2:v=0:a=1"));
    }

    #[test]
    fn compose_image_uses_loop_and_yuv420p() {
        let a = compose_video_args(&p("a.wav"), &p("bg.jpg"), &p("o.mp4"), false, 1920, 1080);
        let j = a.join(" ");
        assert!(j.contains("-loop 1"));
        assert!(j.contains("yuv420p"));
        assert!(j.contains("-shortest"));
        assert!(j.contains("scale=1920:1080"));
    }

    #[test]
    fn compose_video_loops_background_video() {
        let a = compose_video_args(&p("a.wav"), &p("bg.mp4"), &p("o.mp4"), true, 1280, 720);
        assert!(a.join(" ").contains("-stream_loop -1"));
    }

    #[test]
    fn assemble_concats_files_and_silence() {
        let parts = vec![
            AudioPart::Silence(0.5),
            AudioPart::File(p("a.wav")),
            AudioPart::Silence(1.0),
        ];
        let a = assemble_args(&parts, &p("out.wav"));
        let j = a.join(" ");
        assert!(j.contains("anullsrc=r=48000:cl=mono"));
        assert!(j.contains("concat=n=3:v=0:a=1"));
        assert_eq!(a.last().unwrap(), "out.wav");
    }

    #[test]
    fn voice_sequence_inserts_delays_and_skips_missing_parts() {
        let content = [p("c1.wav"), p("c2.wav")];
        let refs: Vec<&std::path::Path> = content.iter().map(|x| x.as_path()).collect();
        let d = VoiceDelays {
            before_intro: 0.5,
            after_intro: 0.8,
            after_content: 0.8,
            after_outro: 1.0,
        };
        // intro + 2 content + outro, with 4 silences interleaved = 8 parts
        let full = voice_sequence(Some(&p("intro.wav")), &refs, Some(&p("outro.wav")), &d);
        assert_eq!(full.len(), 8);
        assert!(matches!(full[0], AudioPart::Silence(_)));
        // no intro/outro → just the content files, no silence
        let bare = voice_sequence(None, &refs, None, &d);
        assert_eq!(bare.len(), 2);
        assert!(bare.iter().all(|p| matches!(p, AudioPart::File(_))));
        // zero delays collapse
        let zero = VoiceDelays {
            before_intro: 0.0,
            after_intro: 0.0,
            after_content: 0.0,
            after_outro: 0.0,
        };
        let s = voice_sequence(Some(&p("intro.wav")), &refs, Some(&p("outro.wav")), &zero);
        assert_eq!(s.len(), 4); // intro + 2 content + outro, no silence
    }

    #[test]
    fn concat_builds_filter_for_n_inputs() {
        let a = concat_audio_args(&[&p("1.wav"), &p("2.wav"), &p("3.wav")], &p("o.wav"));
        assert!(a.join(" ").contains("[0:a][1:a][2:a]concat=n=3:v=0:a=1"));
    }

    #[test]
    fn parses_probe_duration() {
        assert!((parse_duration("123.45\n").unwrap() - 123.45).abs() < 1e-9);
        assert!(parse_duration("nope").is_err());
    }

    #[test]
    fn atempo_chain_stays_within_bounds() {
        // simple factor → single stage
        assert_eq!(atempo_chain(1.5), "atempo=1.5000");
        // > 2.0 splits: 2.5 → 2.0 * 1.25
        assert_eq!(atempo_chain(2.5), "atempo=2.0,atempo=1.2500");
        // < 0.5 splits: 0.4 → 0.5 * 0.8
        assert_eq!(atempo_chain(0.4), "atempo=0.5,atempo=0.8000");
    }

    #[test]
    fn extract_audio_is_mono_16k_wav() {
        let a = extract_audio_args(&p("in.mp4"), &p("out.wav"));
        let j = a.join(" ");
        assert!(j.contains("-ac 1"));
        assert!(j.contains("-ar 16000"));
        assert!(j.contains("-vn"));
    }

    #[test]
    fn mux_dub_lowers_original_and_copies_video() {
        let a = mux_dub_args(&p("v.mp4"), &p("vn.wav"), &p("o.mp4"), 0.15);
        let j = a.join(" ");
        assert!(j.contains("volume=0.150"));
        assert!(j.contains("amix=inputs=2:duration=longest"));
        assert!(j.contains("-c:v copy"));
    }

    #[test]
    fn build_srt_numbers_and_formats() {
        let cues = [
            Cue {
                start: 0.0,
                end: 1.5,
                text: "Xin chào",
                top: None,
            },
            Cue {
                start: 2.0,
                end: 2.0,
                text: "  ",
                top: None,
            }, // empty → skipped
            Cue {
                start: 61.25,
                end: 63.0,
                text: "Tạm biệt",
                top: None,
            },
        ];
        let srt = build_srt(&cues);
        assert!(srt.contains("1\n00:00:00,000 --> 00:00:01,500\nXin chào"));
        assert!(srt.contains("2\n00:01:01,250 --> 00:01:03,000\nTạm biệt"));
        assert!(!srt.contains("3\n")); // only two non-empty cues
    }

    #[test]
    fn build_srt_bilingual_emits_source_over_vietnamese() {
        let cues = [Cue {
            start: 0.0,
            end: 1.5,
            text: "Xin chào",
            top: Some("你好"),
        }];
        let srt = build_srt(&cues);
        // source line sits above the Vietnamese within the same cue block
        assert!(srt.contains("1\n00:00:00,000 --> 00:00:01,500\n你好\nXin chào\n"));
    }

    #[test]
    fn ass_color_converts_hex_to_bgr() {
        // #FFE082 → &H0082E0FF& (B,G,R byte order, alpha 00)
        assert_eq!(ass_color("#FFE082"), "&H0082E0FF&");
        assert_eq!(ass_color("#ffffff"), "&H00FFFFFF&");
        assert_eq!(ass_color("bogus"), "&H00FFFFFF&"); // fallback
    }

    #[test]
    fn export_no_filters_copies_video() {
        let o = ExportOpts {
            original_volume: 0.2,
            vn_volume: 1.0,
            subtitles_burn: None,
            subtitles_soft: None,
            sub_margin_v: None,
            sub_size: None,
            sub_color: None,
            blur: None,
            frame: None,
            overlays: vec![],
            lead_in: 0.0,
        };
        let a = export_video_args(&p("v.mp4"), &p("vn.wav"), &p("o.mp4"), &o);
        let j = a.join(" ");
        assert!(j.contains("-c:v copy"));
        assert!(j.contains("amix=inputs=2:duration=longest")); // audio still mixed
        assert!(j.contains("-map 0:v:0"));
    }

    #[test]
    fn export_with_feathered_blur_and_burned_subs() {
        let sub = p("/tmp/s.srt");
        let o = ExportOpts {
            original_volume: 0.2,
            vn_volume: 1.0,
            subtitles_burn: Some(&sub),
            subtitles_soft: None,
            sub_margin_v: Some(60),
            sub_size: Some(36.0),
            sub_color: Some("#FFE082"),
            blur: Some((0.1, 0.84, 0.8, 0.14)),
            frame: Some((1000, 500)),
            overlays: vec![],
            lead_in: 0.0,
        };
        let a = export_video_args(&p("v.mp4"), &p("vn.wav"), &p("o.mp4"), &o);
        let j = a.join(" ");
        // feathered blur: pixel crop + soft alpha mask + overlay at pixel coords
        assert!(j.contains("crop=800:70:100:420"));
        assert!(j.contains("alphamerge"));
        assert!(j.contains("drawbox=x=13:y=13:w=774:h=44"));
        assert!(j.contains("overlay=100:420"));
        // force_style carries margin + size + colour, entries joined with escaped
        // commas so the filtergraph parser doesn't split them.
        assert!(j.contains("force_style='MarginV=60\\,FontSize=36\\,PrimaryColour=&H0082E0FF&'"));
        assert!(!j.contains(",force_style"));
        assert!(j.contains("-c:v libx264"));
        assert!(j.contains("[vs]"));
    }

    #[test]
    fn export_blur_without_frame_uses_hard_edge_fallback() {
        let o = ExportOpts {
            original_volume: 0.2,
            vn_volume: 1.0,
            subtitles_burn: None,
            subtitles_soft: None,
            sub_margin_v: None,
            sub_size: None,
            sub_color: None,
            blur: Some((0.1, 0.84, 0.8, 0.14)),
            frame: None,
            overlays: vec![],
            lead_in: 0.0,
        };
        let j = export_video_args(&p("v.mp4"), &p("vn.wav"), &p("o.mp4"), &o).join(" ");
        assert!(j.contains("overlay=W*0.1000:H*0.8400"));
        assert!(!j.contains("alphamerge"));
    }

    #[test]
    fn export_soft_subs_embeds_mov_text_and_copies_video() {
        let sub = p("/tmp/s.srt");
        let o = ExportOpts {
            original_volume: 0.2,
            vn_volume: 1.0,
            subtitles_burn: None,
            subtitles_soft: Some(&sub),
            sub_margin_v: None,
            sub_size: None,
            sub_color: None,
            blur: None,
            frame: None,
            overlays: vec![],
            lead_in: 0.0,
        };
        let a = export_video_args(&p("v.mp4"), &p("vn.wav"), &p("o.mp4"), &o);
        let j = a.join(" ");
        assert!(j.contains("-c:v copy")); // no video filter → copy
        assert!(j.contains("-c:s mov_text"));
        assert!(j.contains("-map 2:0")); // the srt is the 3rd input
        assert!(!j.contains("subtitles=")); // not a filter burn
    }

    #[test]
    fn compose_export_builds_black_base_and_bounds_length() {
        let clips: Vec<ClipArg> = vec![];
        let a = compose_export_args(&clips, 12.0, (1920, 1080), &p("o.mp4"), true);
        let j = a.join(" ");
        // black base canvas as lavfi input 0
        assert!(j.contains("color=c=black:s=1920x1080:r=30:d=12.000"));
        // no audio clips → silence so the output still has an audio track
        assert!(j.contains("anullsrc=r=48000:cl=stereo"));
        assert!(j.contains("[1:a]anull[a]"));
        // bounded length + h264/aac encode
        assert!(j.contains("-t 12.000"));
        assert!(j.contains("-c:v libx264"));
        assert!(j.contains("-pix_fmt yuv420p"));
        assert!(j.contains("-c:a aac"));
    }

    #[test]
    fn compose_export_video_clip_trims_scales_and_overlays() {
        let src = p("clip.mp4");
        let clips = vec![ClipArg {
            kind: ClipKind::Video,
            source: Some(&src),
            start: 2.0,
            dur: 3.0,
            in_s: 1.5,
            volume: 1.0,
            x: 0.25,
            y: 0.1,
            w: 0.5,
            opacity: 1.0,
            text: None,
        }];
        let a = compose_export_args(&clips, 6.0, (1000, 600), &p("o.mp4"), true);
        let j = a.join(" ");
        // video input is index 1 (base canvas is 0)
        assert!(
            j.contains("[1:v]trim=start=1.500:duration=3.000,setpts=PTS-STARTPTS,scale=500:-2[v0]")
        );
        // overlaid onto the base with a time gate
        assert!(
            j.contains("[0:v][v0]overlay=W*0.2500:H*0.1000:enable='between(t,2.000,5.000)'[vc0]")
        );
        // final video map is the running base [vc0]
        assert!(j.contains("-map [vc0]"));
    }

    #[test]
    fn compose_export_audio_clip_delays_and_mixes() {
        let src = p("voice.wav");
        let clips = vec![ClipArg {
            kind: ClipKind::Audio,
            source: Some(&src),
            start: 1.25,
            dur: 2.0,
            in_s: 0.0,
            volume: 0.8,
            x: 0.0,
            y: 0.0,
            w: 1.0,
            opacity: 1.0,
            text: None,
        }];
        let j = compose_export_args(&clips, 4.0, (1280, 720), &p("o.mp4"), true).join(" ");
        assert!(j.contains(
            "[1:a]atrim=start=0.000:duration=2.000,asetpts=PTS-STARTPTS,volume=0.800,adelay=1250:all=1[a0]"
        ));
        // single audio clip is still mixed (over the base canvas's absence) → amix of 1
        assert!(j.contains("[a0]amix=inputs=1:normalize=0:duration=longest[a]"));
        assert!(j.contains("-map [a]"));
        // no extra silence input when audio clips exist
        assert!(!j.contains("anullsrc"));
    }

    #[test]
    fn compose_export_image_applies_opacity_and_text_drawtext() {
        let img = p("banner.png");
        let clips = vec![
            ClipArg {
                kind: ClipKind::Image,
                source: Some(&img),
                start: 0.0,
                dur: 5.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.05,
                y: 0.8,
                w: 0.3,
                opacity: 0.5,
                text: None,
            },
            ClipArg {
                kind: ClipKind::Text,
                source: None,
                start: 1.0,
                dur: 2.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: Some("Xin chào: 100%"),
            },
        ];
        let j = compose_export_args(&clips, 6.0, (1000, 1000), &p("o.mp4"), true).join(" ");
        // image: scaled to fractional width, alpha from opacity, overlaid with gate
        assert!(j.contains("[1:v]scale=300:-1,format=rgba,colorchannelmixer=aa=0.500[im0]"));
        assert!(
            j.contains("[0:v][im0]overlay=W*0.0500:H*0.8000:enable='between(t,0.000,5.000)'[vc0]")
        );
        // text: drawn on the running base, colon + percent escaped
        assert!(j.contains("drawtext=text='Xin chào\\: 100\\%'"));
        assert!(j.contains("enable='between(t,1.000,3.000)'[vc1]"));
        assert!(j.contains("-map [vc1]"));
    }

    /// End-to-end: the compositing export must actually produce a file on this
    /// machine's ffmpeg (which lacks `drawtext`) — reproduces the "Đang xuất…
    /// then nothing" bug. Skipped when ffmpeg isn't available.
    #[tokio::test]
    async fn compose_export_renders_end_to_end() {
        if !has_filter("scale").await {
            return; // no usable ffmpeg in this environment
        }
        let dir = std::env::temp_dir().join(format!("compose_it_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let vid = dir.join("src.mp4");
        let out = dir.join("out.mp4");
        let mk: Vec<String> = [
            "-y",
            "-f",
            "lavfi",
            "-t",
            "1",
            "-i",
            "testsrc=s=160x120:r=30",
            "-f",
            "lavfi",
            "-t",
            "1",
            "-i",
            "sine=frequency=440",
            "-shortest",
        ]
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(vid.to_string_lossy().into_owned()))
        .collect();
        run_ffmpeg(&mk).await.expect("make test source");

        let clips = vec![
            ClipArg {
                kind: ClipKind::Video,
                source: Some(&vid),
                start: 0.0,
                dur: 1.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: None,
            },
            ClipArg {
                kind: ClipKind::Audio,
                source: Some(&vid),
                start: 0.0,
                dur: 1.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: None,
            },
            // a text/subtitle clip — must NOT break the render even without drawtext
            ClipArg {
                kind: ClipKind::Text,
                source: None,
                start: 0.0,
                dur: 1.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: Some("Xin chào"),
            },
        ];
        let text_ok = has_filter("drawtext").await;
        let args = compose_export_args(&clips, 1.0, (160, 120), &out, text_ok);
        run_ffmpeg(&args)
            .await
            .expect("compositing export must succeed");
        let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(len > 0, "export produced no output");
    }

    #[test]
    fn compose_export_skips_text_when_drawtext_unavailable() {
        let img = p("logo.png");
        let clips = vec![
            ClipArg {
                kind: ClipKind::Image,
                source: Some(&img),
                start: 0.0,
                dur: 5.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.05,
                y: 0.8,
                w: 0.3,
                opacity: 1.0,
                text: None,
            },
            ClipArg {
                kind: ClipKind::Text,
                source: None,
                start: 1.0,
                dur: 2.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: Some("Xin chào"),
            },
        ];
        // drawtext available → the subtitle is drawn.
        let with = compose_export_args(&clips, 6.0, (1000, 1000), &p("o.mp4"), true).join(" ");
        assert!(with.contains("drawtext"));
        // drawtext unavailable → the text clip is skipped so the export still
        // renders (this build of ffmpeg lacks drawtext); the final map stays valid.
        let without = compose_export_args(&clips, 6.0, (1000, 1000), &p("o.mp4"), false).join(" ");
        assert!(
            !without.contains("drawtext"),
            "text clips must be skipped when drawtext is unavailable"
        );
        assert!(without.contains("-map [vc0]"));
    }

    #[test]
    fn compose_export_multi_video_chains_overlays_in_order() {
        let a0 = p("base.mp4");
        let a1 = p("pip.mp4");
        let clips = vec![
            ClipArg {
                kind: ClipKind::Video,
                source: Some(&a0),
                start: 0.0,
                dur: 10.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.0,
                y: 0.0,
                w: 1.0,
                opacity: 1.0,
                text: None,
            },
            ClipArg {
                kind: ClipKind::Video,
                source: Some(&a1),
                start: 0.0,
                dur: 10.0,
                in_s: 0.0,
                volume: 1.0,
                x: 0.6,
                y: 0.6,
                w: 0.35,
                opacity: 1.0,
                text: None,
            },
        ];
        let j = compose_export_args(&clips, 10.0, (1920, 1080), &p("o.mp4"), true).join(" ");
        // first video overlays onto the lavfi base, second overlays onto the result
        assert!(j.contains("[0:v][v0]overlay=W*0.0000:H*0.0000:"));
        assert!(j.contains("[vc0][v1]overlay=W*0.6000:H*0.6000:"));
        // the last one is the mapped output
        assert!(j.contains("-map [vc1]"));
    }
}
