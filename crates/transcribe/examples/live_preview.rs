//! Runs the live preview off the real microphone and the real model.
//!
//! `cargo run -p aloud-transcribe --example live_preview [seconds]`
//!
//! The unit test feeds a fake transcriber synthetic tone, which proves the
//! plumbing and nothing about the thing this feature exists for: **whether the
//! words keep up with the mouth**. Each line below is stamped with how long its
//! pass took and how far into the recording it landed — if the pass time
//! approaches `COMMIT_SECONDS`, the preview is falling behind and the window is
//! too big for this machine.

use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(20);

    let dir = aloud_transcribe::local::model_dir()?;
    let model = aloud_transcribe::download::ensure_model(
        &dir,
        aloud_transcribe::local::DEFAULT_MODEL,
        |p| {
            if p >= 1.0 {
                println!("model ready");
            }
        },
    )
    .await?;
    let transcriber: Arc<dyn aloud_transcribe::Transcriber> = Arc::new(
        aloud_transcribe::local::LocalTranscriber::new(&model, None)?,
    );

    let started = Instant::now();
    let preview = aloud_transcribe::preview::Preview::spawn(
        &tokio::runtime::Handle::current(),
        async move { Ok(transcriber) },
        Box::new(move |partial| {
            println!("[{:>5.1?}] {partial}", started.elapsed());
        }),
    );

    println!("Recording {seconds}s — speak for longer than a window and watch the text hold.");
    let recorder = aloud_audio::Recorder::start(None, Some(preview.tap()))?;
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    drop(recorder);
    drop(preview);
    Ok(())
}
