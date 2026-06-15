//! ffmpeg command construction for the audio/video pipeline. The argument
//! builders are pure and unit-tested; execution shells out to ffmpeg/ffprobe.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

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

/// Run ffmpeg with the given args (requires `ffmpeg` on PATH).
pub async fn run_ffmpeg(args: &[String]) -> Result<()> {
    let status = tokio::process::Command::new("ffmpeg")
        .args(args)
        .status()
        .await
        .context("spawn ffmpeg (is it installed?)")?;
    if !status.success() {
        return Err(anyhow!("ffmpeg exited with {status}"));
    }
    Ok(())
}

/// Probe a media file's duration in seconds (requires `ffprobe` on PATH).
pub async fn probe_duration(path: &Path) -> Result<f64> {
    let out = tokio::process::Command::new("ffprobe")
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
}
