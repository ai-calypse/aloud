//! Fetches whisper models from Hugging Face on first use.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Downloads `model` into `dir` unless it is already there, reporting progress
/// as a 0.0–1.0 fraction. Returns the model path.
pub async fn ensure_model(
    dir: &Path,
    model: &str,
    mut on_progress: impl FnMut(f32),
) -> Result<PathBuf> {
    let path = dir.join(model);
    if path.exists() {
        return Ok(path);
    }
    tokio::fs::create_dir_all(dir)
        .await
        .with_context(|| format!("creating {}", dir.display()))?;

    let url = format!("{MODEL_BASE_URL}/{model}");
    tracing::info!(%url, "downloading whisper model");

    let response = reqwest::get(&url)
        .await
        .with_context(|| format!("requesting {url}"))?;
    if !response.status().is_success() {
        bail!("model download failed ({}): {url}", response.status());
    }
    let total = response.content_length();

    // Download to a temp path and rename on success, so an interrupted
    // download can never be mistaken for a usable model.
    let temp = path.with_extension("partial");
    let mut file = tokio::fs::File::create(&temp)
        .await
        .with_context(|| format!("creating {}", temp.display()))?;

    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("downloading model")?;
        file.write_all(&chunk).await.context("writing model")?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total.filter(|t| *t > 0) {
            on_progress(downloaded as f32 / total as f32);
        }
    }
    file.flush().await.context("flushing model")?;
    drop(file);

    tokio::fs::rename(&temp, &path)
        .await
        .context("finalizing model download")?;
    on_progress(1.0);

    tracing::info!(path = %path.display(), bytes = downloaded, "model ready");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn existing_model_is_not_redownloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.bin");
        tokio::fs::write(&path, b"cached").await.unwrap();

        let mut called = false;
        let result = ensure_model(dir.path(), "model.bin", |_| called = true)
            .await
            .unwrap();

        assert_eq!(result, path);
        assert!(!called, "must not download when the model is cached");
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"cached");
    }
}
