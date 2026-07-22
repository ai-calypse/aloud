//! Drives the meeting chunker from the real microphone tap — the part a unit
//! test cannot assert: the carried remainder surviving thousands of real
//! callbacks at device-chosen buffer sizes.
//!
//! `cargo run -p aloud-audio --example meeting_chunks`
//!
//! 2-second chunks rather than the real 60; the boundary arithmetic is the same.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const CHUNK_SECONDS: usize = 2;
const RECORD_SECONDS: u64 = 7;

fn main() -> anyhow::Result<()> {
    let bytes_per_second = aloud_audio::REALTIME_RATE as usize * 2; // i16
    let chunk_bytes = bytes_per_second * CHUNK_SECONDS;

    let chunker = Arc::new(Mutex::new(aloud_audio::Chunker::new(chunk_bytes)));
    let chunks = Arc::new(Mutex::new(Vec::<(usize, Vec<u8>)>::new()));
    // Printed rather than assumed — the buffer size is the device's to choose.
    let buffers = Arc::new(Mutex::new(Vec::<usize>::new()));

    let (tap_chunker, tap_chunks, tap_buffers) = (chunker.clone(), chunks.clone(), buffers.clone());

    println!("Recording {RECORD_SECONDS}s — speak, so the chunks are not silence.");
    let recorder = aloud_audio::Recorder::start(
        None,
        Some(Box::new(move |bytes: Vec<u8>| {
            tap_buffers.lock().unwrap().push(bytes.len());
            let mut chunker = tap_chunker.lock().unwrap();
            for chunk in chunker.push(&bytes) {
                tap_chunks.lock().unwrap().push(chunk);
            }
        })),
    )?;

    std::thread::sleep(Duration::from_secs(RECORD_SECONDS));
    let _ = recorder.stop();

    let (tail_index, tail) = chunker.lock().unwrap().take_tail();
    let chunks = chunks.lock().unwrap();
    let buffers = buffers.lock().unwrap();

    let distinct: std::collections::BTreeSet<_> = buffers.iter().copied().collect();
    println!(
        "\n{} tap buffers, sizes seen: {:?}",
        buffers.len(),
        distinct
    );
    println!("chunk size: {chunk_bytes} bytes ({CHUNK_SECONDS}s)\n");

    for (index, chunk) in chunks.iter() {
        let path = std::env::temp_dir().join(format!("aloud-meeting-chunk-{index}.wav"));
        aloud_audio::write_pcm16_wav(&path, chunk)?;
        let mut reader = hound::WavReader::open(&path)?;
        let spec = reader.spec();
        let peak = reader
            .samples::<i16>()
            .map(|s| s.unwrap().unsigned_abs())
            .max()
            .unwrap_or(0);
        println!(
            "chunk {index}: {} bytes -> {} Hz, {} ch, peak {peak}  {}",
            chunk.len(),
            spec.sample_rate,
            spec.channels,
            path.display()
        );
    }
    println!(
        "tail: index {tail_index}, {} bytes (unfinished)",
        tail.len()
    );

    // Every tap byte is in a chunk or the tail, exactly once. A remainder bug
    // shows up here and nowhere else.
    let total_in: usize = buffers.iter().sum();
    let total_out: usize = chunks.iter().map(|(_, c)| c.len()).sum::<usize>() + tail.len();
    println!("\ncaptured {total_in} bytes, accounted {total_out}");
    assert_eq!(total_in, total_out, "the chunker lost or duplicated audio");

    assert!(
        chunks.iter().enumerate().all(|(n, (i, _))| n == *i),
        "chunk indexes must be consecutive from zero"
    );
    assert!(
        chunks.iter().all(|(_, c)| c.len() == chunk_bytes),
        "every emitted chunk must be exactly one chunk long"
    );
    assert_eq!(tail_index, chunks.len(), "the tail follows the last chunk");

    println!("ok: no audio lost, indexes consecutive, every chunk exact");
    Ok(())
}
