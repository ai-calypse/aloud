//! Drives the always-listening segmenter from the real mic: what a unit test
//! cannot tell is whether 0.02 RMS separates this mic's speech from this
//! room's quiet. Speak a few phrases with pauses; each should print once.
//!
//! `cargo run -p aloud-audio --example listen_segments [threshold]`

use std::{sync::Mutex, time::Duration};

const RECORD_SECONDS: u64 = 20;

fn main() -> anyhow::Result<()> {
    let threshold = std::env::args().nth(1).map_or(Ok(0.02), |t| t.parse())?;
    let segmenter = Mutex::new(aloud_audio::Segmenter::new(threshold));
    let bytes_per_second = aloud_audio::REALTIME_RATE as f32 * 2.0;

    let _recorder = aloud_audio::Recorder::tap_only(
        None,
        Box::new(move |pcm: Vec<u8>| {
            for utterance in segmenter.lock().unwrap().push(&pcm) {
                println!(
                    "utterance: {:.2}s",
                    utterance.len() as f32 / bytes_per_second
                );
            }
        }),
    )?;
    println!("listening for {RECORD_SECONDS}s at threshold {threshold} — talk, pause, repeat");
    std::thread::sleep(Duration::from_secs(RECORD_SECONDS));
    Ok(())
}
