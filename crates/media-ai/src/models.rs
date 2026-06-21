//! Model files are fetched from the Hugging Face hub on first run and cached —
//! the same pattern vieneu-server uses, so installers stay small.

use anyhow::{Context, Result};
use std::path::PathBuf;

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
