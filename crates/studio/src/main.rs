//! studio-server: HTTP API + background worker for the audiobook pipeline.
//! All settings (API keys, service URLs, render profile) are stored in the DB
//! and edited from the app's Settings page — not via CLI/env.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use studio::config::AppConfig;
use studio::db::Db;
use studio::nodes::{register_default, Services};
use studio::server::{app, run_worker, AppState};
use studio::workflow::Registry;

#[derive(Parser, Debug)]
#[command(
    name = "studio-server",
    about = "Webnovel → audiobook → YouTube automation"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8090")]
    addr: String,
    /// SQLite database path.
    #[arg(long, default_value = "studio.db")]
    db: String,
    /// Working directory for rendered audio/video.
    #[arg(long, default_value = "studio-work")]
    work_dir: String,
}

/// When launched by the desktop shell (`DIE_WITH_PARENT=1` + a piped stdin), exit
/// as soon as that stdin closes — i.e. when the parent dies — so we never linger
/// holding the port. No-op for standalone runs (env unset).
fn die_with_parent() {
    if std::env::var_os("DIE_WITH_PARENT").is_none() {
        return;
    }
    std::thread::spawn(|| {
        use std::io::Read;
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => std::process::exit(0),
                Ok(_) => {}
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    die_with_parent();
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "studio=info,tower_http=info".into()),
        )
        .init();
    let args = Args::parse();

    let work_dir = std::path::PathBuf::from(&args.work_dir);
    let cache_dir = work_dir.join("tts-cache");
    std::fs::create_dir_all(&cache_dir)?;

    let db = Db::connect(&args.db).await?;

    // Load persisted config (from the Settings page), else defaults.
    let mut config: AppConfig = match db.load_config_json().await? {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => AppConfig::default(),
    };
    // When launched by the desktop app, the other sidecars run on dynamically
    // chosen ports passed via VIENEU_BASE / MEDIA_AI_BASE — let those override the
    // stored/default bases (the user no longer sets ports in Settings).
    if let Ok(base) = std::env::var("VIENEU_BASE") {
        if !base.is_empty() {
            config.tts_base = base;
        }
    }
    if let Ok(base) = std::env::var("MEDIA_AI_BASE") {
        if !base.is_empty() {
            config.media_ai_base = base;
        }
    }
    if config.ruin_key.is_empty() {
        tracing::warn!("Ruin API key not set — configure it in the app's Settings page");
    }

    // Seed the bundled CC-BY voice pack (idempotent; non-fatal on failure).
    if let Err(e) = studio::clones::seed::seed_builtin_voices(&db, &work_dir).await {
        tracing::warn!("voicepack seed failed: {e:#}");
    }

    let services = Arc::new(Services {
        db,
        config: tokio::sync::RwLock::new(config),
        work_dir,
        cache_dir,
    });

    let mut registry = Registry::new();
    register_default(&mut registry, services.clone());
    let registry = Arc::new(registry);

    let running: studio::server::RunningMap = Arc::new(std::sync::Mutex::new(Default::default()));
    tokio::spawn(run_worker(
        services.clone(),
        registry.clone(),
        running.clone(),
    ));

    let state = AppState {
        services,
        registry,
        running,
    };
    let listener = tokio::net::TcpListener::bind(&args.addr).await?;
    tracing::info!("studio-server listening on http://{}", args.addr);
    axum::serve(listener, app(state)).await?;
    Ok(())
}
