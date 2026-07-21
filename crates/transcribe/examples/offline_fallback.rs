//! Proves the offline fallback against real failures and real hardware.
//!
//! `cargo run -p aloud-transcribe --example offline_fallback -- <file.wav>`
//!
//! Runs the same two steps `Pipeline::transcribe_with_local_fallback` does —
//! classify what the cloud call returned, then retry on-device — against a
//! base URL nothing is listening on. A unit test cannot show that the local
//! engine still produces text once the cloud one is unreachable, which is the
//! whole claim.

use aloud_transcribe::{cloud::CloudTranscriber, download, is_recoverable_locally, local};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let wav: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/aloud-test.wav".into())
        .into();

    // Port 1 refuses connections, so the failure is a genuine transport error
    // and not a mock of one.
    let cloud = CloudTranscriber::new("k", Some("http://127.0.0.1:1"), None, None)?;
    let error = aloud_transcribe::Transcriber::transcribe(&cloud, &wav)
        .await
        .expect_err("nothing is listening on port 1");
    println!("Cloud failed: {error:#}");
    println!("Recoverable on-device: {}", is_recoverable_locally(&error));
    assert!(
        is_recoverable_locally(&error),
        "the fallback would not fire"
    );

    let dir = local::model_dir()?;
    let model = download::ensure_model(&dir, local::DEFAULT_MODEL, |_| {}).await?;
    let started = std::time::Instant::now();
    let transcriber = local::LocalTranscriber::new(&model, None)?;
    let text = aloud_transcribe::Transcriber::transcribe(&transcriber, &wav).await?;
    println!("Local transcript in {:?}: {text:?}", started.elapsed());
    assert!(!text.trim().is_empty(), "the fallback produced no text");
    Ok(())
}
