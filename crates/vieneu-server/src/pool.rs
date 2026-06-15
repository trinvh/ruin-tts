//! A pool of `Engine` instances for parallel synthesis.
//!
//! Each engine is a heavy, single-owner object. We keep `n` of them in an
//! mpmc channel; a request checks one out, runs the (CPU-bound) inference on a
//! blocking thread, then returns it. This gives true parallelism across
//! chapters/requests — the main win over the GIL-bound Python original.

use std::sync::Arc;

use anyhow::{Context, Result};
use async_channel::{Receiver, Sender};
use vieneu_core::{Engine, ModelSource};

#[derive(Clone, serde::Serialize)]
pub struct VoiceInfo {
    pub id: String,
    pub label: String,
}

pub struct EnginePool {
    tx: Sender<Engine>,
    rx: Receiver<Engine>,
    pub sample_rate: u32,
    pub voices: Vec<VoiceInfo>,
    pub size: usize,
}

impl EnginePool {
    /// Build `n` engines up front (model load is the slow part, done once each).
    pub fn build(
        n: usize,
        source: ModelSource,
        hf_token: Option<String>,
        threads: usize,
    ) -> Result<Arc<Self>> {
        let n = n.max(1);
        let (tx, rx) = async_channel::bounded(n);
        let mut sample_rate = 48_000;
        let mut voices = Vec::new();

        for i in 0..n {
            tracing::info!("loading engine {}/{}", i + 1, n);
            let engine = Engine::load(&source, hf_token.as_deref(), threads, None)
                .with_context(|| format!("load engine {i}"))?;
            if i == 0 {
                sample_rate = engine.sample_rate();
                voices = engine
                    .voices()
                    .list()
                    .into_iter()
                    .map(|(label, id)| VoiceInfo { id, label })
                    .collect();
            }
            tx.send_blocking(engine).context("seed pool")?;
        }

        Ok(Arc::new(Self {
            tx,
            rx,
            sample_rate,
            voices,
            size: n,
        }))
    }

    /// Check out an engine, run `f` on a blocking thread, return the engine.
    pub async fn with_engine<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Engine) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let mut engine = self.rx.recv().await.context("acquire engine")?;
        let tx = self.tx.clone();
        let (engine, result) = tokio::task::spawn_blocking(move || {
            let r = f(&mut engine);
            (engine, r)
        })
        .await
        .context("inference task panicked")?;
        // Return the engine to the pool (ignore if the pool is closing).
        let _ = tx.send(engine).await;
        result
    }
}
