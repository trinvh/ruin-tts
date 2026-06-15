//! In-memory job index for long synthesis tasks. Audio bytes are NOT kept in
//! memory — finished jobs reference a file on disk (the server's temp dir).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde::Serialize;

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

struct Job {
    status: JobStatus,
    cancel: Arc<AtomicBool>,
    path: Option<PathBuf>,
    content_type: &'static str,
    extension: &'static str,
    duration_s: Option<f32>,
    error: Option<String>,
}

/// Public, serializable snapshot of a job (no audio bytes).
#[derive(Serialize)]
pub struct JobView {
    pub status: JobStatus,
    pub duration_s: Option<f32>,
    pub error: Option<String>,
    pub ready: bool,
    /// Absolute path of the finished audio file (same machine), when ready.
    pub path: Option<String>,
}

#[derive(Default)]
pub struct JobStore {
    jobs: Mutex<HashMap<String, Job>>,
}

impl JobStore {
    /// Create a queued job; returns its id and a cancel flag to thread into the
    /// engine call.
    pub fn create(&self) -> (String, Arc<AtomicBool>) {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.lock().unwrap().insert(
            id.clone(),
            Job {
                status: JobStatus::Queued,
                cancel: cancel.clone(),
                path: None,
                content_type: "application/octet-stream",
                extension: "bin",
                duration_s: None,
                error: None,
            },
        );
        (id, cancel)
    }

    pub fn mark_running(&self, id: &str) {
        if let Some(j) = self.jobs.lock().unwrap().get_mut(id) {
            if j.status == JobStatus::Queued {
                j.status = JobStatus::Running;
            }
        }
    }

    pub fn mark_done(
        &self,
        id: &str,
        path: PathBuf,
        content_type: &'static str,
        extension: &'static str,
        duration_s: f32,
    ) {
        if let Some(j) = self.jobs.lock().unwrap().get_mut(id) {
            // A cancel that landed after completion still wins (file cleaned up).
            if j.status == JobStatus::Cancelled {
                let _ = std::fs::remove_file(&path);
                return;
            }
            j.status = JobStatus::Done;
            j.path = Some(path);
            j.content_type = content_type;
            j.extension = extension;
            j.duration_s = Some(duration_s);
        }
    }

    pub fn mark_failed(&self, id: &str, err: String) {
        if let Some(j) = self.jobs.lock().unwrap().get_mut(id) {
            if j.status != JobStatus::Cancelled {
                j.status = JobStatus::Failed;
                j.error = Some(err);
            }
        }
    }

    /// Signal cancellation; flips the flag the engine polls and marks the job.
    pub fn request_cancel(&self, id: &str) -> bool {
        if let Some(j) = self.jobs.lock().unwrap().get_mut(id) {
            j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            if matches!(j.status, JobStatus::Queued | JobStatus::Running) {
                j.status = JobStatus::Cancelled;
            }
            true
        } else {
            false
        }
    }

    pub fn view(&self, id: &str) -> Option<JobView> {
        let map = self.jobs.lock().unwrap();
        let j = map.get(id)?;
        Some(JobView {
            status: j.status,
            duration_s: j.duration_s,
            error: j.error.clone(),
            ready: j.status == JobStatus::Done && j.path.is_some(),
            path: j.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        })
    }

    /// Path + content-type + extension of a finished job's audio file.
    pub fn audio_path(&self, id: &str) -> Option<(PathBuf, &'static str, &'static str)> {
        let map = self.jobs.lock().unwrap();
        let j = map.get(id)?;
        if j.status == JobStatus::Done {
            j.path.clone().map(|p| (p, j.content_type, j.extension))
        } else {
            None
        }
    }
}
