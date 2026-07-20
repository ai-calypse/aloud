//! How long after `Recorder::start` returns does audio actually arrive?
//!
//! `cargo run -p aloud-audio --example first_sample_latency`
//!
//! This is the window in which a user is told "recording" — pill up, chime
//! played — while nothing is being captured. Anything spoken inside it is
//! lost, which is what clips the first word of a short utterance.
//!
//! Run it twice in a row: the first open of the microphone is the expensive
//! one, because CoreAudio has to start the device.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    for attempt in 1..=3 {
        let first_sample_us = Arc::new(AtomicU64::new(0));
        let sink = first_sample_us.clone();

        let began = Instant::now();
        let tap = Box::new(move |_pcm: Vec<u8>| {
            // Only the first callback matters; later ones must not overwrite.
            let _ = sink.compare_exchange(
                0,
                began.elapsed().as_micros() as u64,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        });

        let recorder = aloud_audio::Recorder::start(None, Some(tap))?;
        let returned_us = began.elapsed().as_micros() as u64;

        std::thread::sleep(Duration::from_millis(1500));
        let samples = recorder.stop()?;

        let first = first_sample_us.load(Ordering::SeqCst);
        println!(
            "attempt {attempt}: start() returned after {:.1}ms, first audio at {:.1}ms \
             (dead window {:.1}ms), {} samples captured",
            returned_us as f64 / 1000.0,
            first as f64 / 1000.0,
            (first.saturating_sub(returned_us)) as f64 / 1000.0,
            samples.len(),
        );

        std::thread::sleep(Duration::from_millis(400));
    }

    println!(
        "\nThe dead window is the gap between the user being told to speak \
         and the microphone delivering anything."
    );
    Ok(())
}
