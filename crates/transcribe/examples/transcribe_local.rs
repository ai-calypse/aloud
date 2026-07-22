//! Downloads the default model (once) and transcribes a WAV with no network.
//!
//! `cargo run -p aloud-transcribe --example transcribe_local -- <file.wav>`

use aloud_transcribe::{download, local};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let wav: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/aloud-test.wav".into())
        .into();

    let dir = local::model_dir()?;
    println!("Model dir: {}", dir.display());

    // Every 10%, not every chunk: \r progress is unreadable off a terminal.
    let mut last_decile = 0;
    let model = download::ensure_model(&dir, local::DEFAULT_MODEL, |p| {
        let decile = (p * 10.0) as u32;
        if decile > last_decile {
            last_decile = decile;
            println!("Downloading model: {:.0}%", p * 100.0);
        }
    })
    .await?;
    println!("Model ready: {}", model.display());

    let started = std::time::Instant::now();
    let transcriber = local::LocalTranscriber::new(&model, None)?;
    println!("Loaded in {:?}", started.elapsed());

    let started = std::time::Instant::now();
    let text = aloud_transcribe::Transcriber::transcribe(&transcriber, &wav).await?;
    println!("Transcribed in {:?}", started.elapsed());
    println!("\nTranscript: {text:?}");
    Ok(())
}
