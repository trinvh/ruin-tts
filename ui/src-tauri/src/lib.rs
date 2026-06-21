//! Tauri shell. Launches two sidecar servers — `vieneu-server` (TTS) and
//! `studio-server` (webnovel→audiobook→YouTube automation) — and exposes their
//! base URLs to the frontend, which talks to them over HTTP. Children are
//! stopped on exit; generated files are kept on disk.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;

use tauri::{Manager, RunEvent};

const TTS_ADDR: &str = "127.0.0.1:8080";
const STUDIO_ADDR: &str = "127.0.0.1:8090";

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
        .invoke_handler(tauri::generate_handler![server_base, studio_base, default_output_dir, copy_file])
        .setup(|app| {
            let mut kids = Vec::new();
            if let Some(c) = spawn("VIENEU_SERVER_BIN", "vieneu-server", &["--addr", TTS_ADDR, "--workers", "2"]) {
                kids.push(c);
            } else {
                eprintln!("[tauri] vieneu-server not found — start it manually on {TTS_ADDR}");
            }
            // studio-server inherits RUIN_API_KEY / VIENEU_BASE / YT_* from the env.
            if let Some(c) = spawn("STUDIO_SERVER_BIN", "studio-server", &["--addr", STUDIO_ADDR]) {
                kids.push(c);
            } else {
                eprintln!("[tauri] studio-server not found — start it manually on {STUDIO_ADDR}");
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
