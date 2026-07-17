//! Records 3 seconds from the default mic to `/tmp/aloud-test.wav`.
//!
//! Hardware check that unit tests can't cover: `cargo run -p aloud-audio --example record`

use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("Devices:");
    for device in aloud_audio::input_devices()? {
        println!("  {} ({})", device.name, device.id);
    }

    let tapped = Arc::new(AtomicUsize::new(0));
    let counter = tapped.clone();

    println!("\nRecording 3s...");
    let recorder = aloud_audio::Recorder::start(
        None,
        Some(Box::new(move |chunk: Vec<u8>| {
            counter.fetch_add(chunk.len(), Ordering::Relaxed);
        })),
    )?;

    std::thread::sleep(Duration::from_secs(3));

    let path = Path::new("/tmp/aloud-test.wav");
    recorder.stop_to_wav(path)?;

    let bytes = tapped.load(Ordering::Relaxed);
    let expected = 3 * aloud_audio::REALTIME_RATE as usize * 2; // 2 bytes per PCM16 sample
    println!("Wrote {}", path.display());
    println!("Realtime tap: {bytes} bytes (~{expected} expected at 24kHz PCM16)");
    Ok(())
}
