//! Streams a real dictation off the real microphone, and A/Bs it against the
//! one-shot path it replaces.
//!
//! `cargo run -p aloud-transcribe --example stream_dictation [seconds]`
//!
//! Unit tests feed the stream a fake transcriber and synthetic buffers, which
//! proves the plumbing and nothing about the thing this feature exists for:
//! **how long the user waits after letting go of the key**, and whether the
//! text that arrives is still worth pasting once whisper has seen it five
//! seconds at a time instead of all at once. Both numbers need a microphone
//! and a real model, so both live here.
//!
//! Prints, for the same recording:
//!   - streamed: the wait after "release", and the transcript
//!   - one-shot: the same two, transcribing the whole WAV as today's pipeline does

use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(12);

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

    let stream = aloud_transcribe::stream::Stream::spawn(
        &tokio::runtime::Handle::current(),
        {
            let t = transcriber.clone();
            async move { Ok(t) }
        },
        Box::new(|partial| println!("  partial: {partial:?}")),
    );

    println!(
        "Recording {seconds}s in {}s chunks — speak a few full sentences.",
        aloud_transcribe::stream::CHUNK_SECONDS
    );
    let recorder = aloud_audio::Recorder::start(None, Some(stream.tap()))?;
    tokio::time::sleep(Duration::from_secs(seconds)).await;

    // The moment the user lets go. Everything after this is the wait the
    // feature exists to shorten.
    let released = Instant::now();
    let wav = std::env::temp_dir().join("aloud-stream-example.wav");
    recorder.stop_to_wav(&wav)?;

    let streamed = stream.finish().await;
    let streamed_wait = released.elapsed();

    let started = Instant::now();
    let one_shot = aloud_transcribe::Transcriber::transcribe(transcriber.as_ref(), &wav).await?;
    let one_shot_wait = started.elapsed();
    let _ = std::fs::remove_file(&wav);

    println!("\n--- after release ---");
    println!("streamed  {streamed_wait:>8.2?}  {streamed:?}");
    println!("one-shot  {one_shot_wait:>8.2?}  {one_shot:?}");
    println!(
        "\nRead the two transcripts, not just the two times: the chunk cuts land\n\
         on the clock, so a word straddling one is where this loses."
    );
    Ok(())
}
