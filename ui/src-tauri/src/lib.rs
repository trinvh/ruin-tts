//! Tauri shell. Launches two sidecar servers — `vieneu-server` (TTS) and
//! `studio-server` (webnovel→audiobook→YouTube automation) — and exposes their
//! base URLs to the frontend, which talks to them over HTTP. Children are
//! stopped on exit; generated files are kept on disk.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;

use tauri::{Manager, RunEvent};

const FFMPEG_NAME: &str = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
const FFPROBE_NAME: &str = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };

const TTS_ADDR: &str = "127.0.0.1:8080";
const STUDIO_ADDR: &str = "127.0.0.1:8090";
const MEDIA_AI_ADDR: &str = "127.0.0.1:8099";

#[derive(Default)]
struct Children(Mutex<Vec<Child>>);

#[tauri::command]
fn server_base() -> String {
    format!("http://{TTS_ADDR}")
}

#[tauri::command]
fn studio_base() -> String {
    format!("http://{STUDIO_ADDR}")
}

#[tauri::command]
fn default_output_dir() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned()
}

#[tauri::command]
fn copy_file(src: String, dest: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

// ── First-run onboarding: ffmpeg detection + download ───────────────────────
fn ffmpeg_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("bin")
}
/// (ffmpeg, ffprobe) target paths inside the app's bin dir.
fn ffmpeg_paths(app: &tauri::AppHandle) -> (PathBuf, PathBuf) {
    let d = ffmpeg_dir(app);
    (d.join(FFMPEG_NAME), d.join(FFPROBE_NAME))
}
fn ffmpeg_on_path() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Is ffmpeg usable (downloaded into the app, or on PATH)? The onboarding UI uses
/// this to decide whether to offer the download.
#[tauri::command]
fn ffmpeg_status(app: tauri::AppHandle) -> serde_json::Value {
    let (ff, fp) = ffmpeg_paths(&app);
    let bundled = ff.exists() && fp.exists();
    let system = ffmpeg_on_path();
    serde_json::json!({
        "available": bundled || system,
        "bundled": bundled,
        "system": system,
        "downloadable": ffmpeg_base_url().is_some(),
    })
}

/// Static-build base for this platform (ffmpeg.martin-riedl.de), or None.
fn ffmpeg_base_url() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release"),
        ("macos", "x86_64") => Some("https://ffmpeg.martin-riedl.de/redirect/latest/macos/amd64/release"),
        ("windows", "x86_64") => Some("https://ffmpeg.martin-riedl.de/redirect/latest/windows/amd64/release"),
        ("linux", "x86_64") => Some("https://ffmpeg.martin-riedl.de/redirect/latest/linux/amd64/release"),
        _ => None,
    }
}

/// Download static ffmpeg + ffprobe for this platform into the app bin dir.
#[tauri::command]
fn download_ffmpeg(app: tauri::AppHandle) -> Result<(), String> {
    let dir = ffmpeg_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let base = ffmpeg_base_url().ok_or("nền tảng này chưa hỗ trợ tải ffmpeg tự động")?;
    let (ff, fp) = ffmpeg_paths(&app);
    fetch_zip_binary(&format!("{base}/ffmpeg.zip"), &ff)?;
    fetch_zip_binary(&format!("{base}/ffprobe.zip"), &fp)?;
    Ok(())
}

/// Download a zip over HTTPS and extract its single binary entry to `dest`.
fn fetch_zip_binary(url: &str, dest: &PathBuf) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| format!("tải {url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(buf)).map_err(|e| format!("giải nén: {e}"))?;
    let mut file = archive.by_index(0).map_err(|e| e.to_string())?;
    let mut out = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Candidate locations for a sidecar binary, in priority order. The platform
/// executable suffix (`.exe` on Windows, empty elsewhere) is appended so the
/// bundled/built binaries are found on every OS.
fn candidates(env_var: &str, name: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var(env_var) {
        v.push(p.into());
    }
    let exe_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join(&exe_name));
        }
    }
    v.push(PathBuf::from(format!("{}/../../target/release/{exe_name}", env!("CARGO_MANIFEST_DIR"))));
    v
}

fn spawn(env_var: &str, name: &str, args: &[&str]) -> Option<Child> {
    let bin = candidates(env_var, name).into_iter().find(|p| p.exists())?;
    eprintln!("[tauri] launching {name}: {}", bin.display());
    Command::new(bin)
        .args(args)
        .spawn()
        .map_err(|e| eprintln!("[tauri] failed to spawn {name}: {e}"))
        .ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Children::default())
        .invoke_handler(tauri::generate_handler![
            server_base,
            studio_base,
            default_output_dir,
            copy_file,
            ffmpeg_status,
            download_ffmpeg
        ])
        .setup(|app| {
            let mut kids = Vec::new();
            // Point studio-server at the app-managed ffmpeg (used iff it exists —
            // see media::ffmpeg_bin); a download during onboarding makes it appear.
            let (ff, fp) = ffmpeg_paths(app.handle());
            std::env::set_var("FFMPEG_PATH", &ff);
            std::env::set_var("FFPROBE_PATH", &fp);
            if let Some(c) = spawn("VIENEU_SERVER_BIN", "vieneu-server", &["--addr", TTS_ADDR, "--workers", "2"]) {
                kids.push(c);
            } else {
                eprintln!("[tauri] vieneu-server not found — start it manually on {TTS_ADDR}");
            }
            // A bundled app's CWD is `/` (macOS) — give studio-server absolute
            // db + work paths under the OS app-data dir so it doesn't write junk.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let _ = std::fs::create_dir_all(&data_dir);
            let db = data_dir.join("studio.db").to_string_lossy().into_owned();
            let work = data_dir.join("studio-work").to_string_lossy().into_owned();
            // studio-server inherits RUIN_API_KEY / VIENEU_BASE / YT_* from the env.
            if let Some(c) = spawn("STUDIO_SERVER_BIN", "studio-server", &["--addr", STUDIO_ADDR, "--db", &db, "--work-dir", &work]) {
                kids.push(c);
            } else {
                eprintln!("[tauri] studio-server not found — start it manually on {STUDIO_ADDR}");
            }
            // media-ai (ASR + diarization + age/gender) — downloads its models on
            // first run. Optional age/gender model via MEDIA_AI_AGEGENDER_* env.
            if let Some(c) = spawn("MEDIA_AI_BIN", "media-ai", &["--addr", MEDIA_AI_ADDR]) {
                kids.push(c);
            } else {
                eprintln!("[tauri] media-ai not found — dubbing analysis unavailable on {MEDIA_AI_ADDR}");
            }
            *app.state::<Children>().0.lock().unwrap() = kids;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                for mut child in app.state::<Children>().0.lock().unwrap().drain(..) {
                    let _ = child.kill();
                }
            }
        });
}
