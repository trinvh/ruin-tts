//! Model files are fetched from the Hugging Face hub on first run and cached —
//! the same pattern vieneu-server uses, so installers stay small. Downloads can
//! report byte progress (for the desktop onboarding) via [`DownloadProgress`].

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The `/progress` payload: aggregate bytes across all files seen so far.
#[derive(Default, Clone, serde::Serialize)]
pub struct DownloadStatus {
    pub file: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
}

#[derive(Default)]
struct Inner {
    current: String,
    /// Per-file (downloaded, total) — keyed so hf-hub re-calling `init` for the
    /// same file (retry/resume) overwrites rather than double-counts.
    files: HashMap<String, (u64, u64)>,
    done: bool,
}

#[derive(Clone, Default)]
pub struct DownloadProgress(Arc<Mutex<Inner>>);

impl DownloadProgress {
    pub fn snapshot(&self) -> DownloadStatus {
        let s = self.0.lock().unwrap();
        DownloadStatus {
            file: s.current.clone(),
            downloaded: s.files.values().map(|(d, _)| *d).sum(),
            total: s.files.values().map(|(_, t)| *t).sum(),
            done: s.done,
        }
    }
    pub fn set_done(&self) {
        self.0.lock().unwrap().done = true;
    }
}

/// hf-hub progress callback for one file; folds into the shared per-file map.
struct FileProgress {
    shared: Arc<Mutex<Inner>>,
    name: String,
}
impl hf_hub::api::Progress for FileProgress {
    fn init(&mut self, size: usize, filename: &str) {
        self.name = filename.rsplit('/').next().unwrap_or(filename).to_string();
        let mut s = self.shared.lock().unwrap();
        s.current = self.name.clone();
        // (re)set this file's total; reset its downloaded so a retry doesn't
        // double-count.
        s.files.insert(self.name.clone(), (0, size as u64));
    }
    fn update(&mut self, size: usize) {
        let mut s = self.shared.lock().unwrap();
        if let Some(e) = s.files.get_mut(&self.name) {
            e.0 += size as u64;
        }
    }
    fn finish(&mut self) {
        let mut s = self.shared.lock().unwrap();
        if let Some(e) = s.files.get_mut(&self.name) {
            e.0 = e.1; // clamp to total
        }
    }
}

/// Fetch a file from the hub (cached). No progress reporting.
pub fn hf_file(repo: &str, file: &str, token: Option<String>) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new()
        .with_token(token)
        .build()
        .context("khởi tạo hf-hub api")?;
    api.model(repo.to_string())
        .get(file)
        .with_context(|| format!("tải {repo}/{file}"))
}

/// Fetch a file, reporting byte progress into `prog`. Cache-aware: an already-
/// downloaded file is returned instantly without re-downloading.
pub fn hf_file_with_progress(
    repo: &str,
    file: &str,
    token: Option<String>,
    prog: &DownloadProgress,
) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;
    use hf_hub::{Cache, Repo, RepoType};
    if let Some(p) = Cache::default()
        .repo(Repo::new(repo.to_string(), RepoType::Model))
        .get(file)
    {
        return Ok(p);
    }
    let api = ApiBuilder::new()
        .with_token(token)
        .with_progress(false)
        .build()
        .context("khởi tạo hf-hub api")?;
    api.model(repo.to_string())
        .download_with_progress(
            file,
            FileProgress {
                shared: prog.0.clone(),
                name: String::new(),
            },
        )
        .with_context(|| format!("tải {repo}/{file}"))
}
