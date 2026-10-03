//! Streams to Deepgram Flux and prints what it hears — the check a unit test
//! cannot make: that the key, the socket and 24 kHz audio are accepted, and a
//! spoken request comes back as one turn.
//!
//! `DEEPGRAM_API_KEY=… cargo run -p aloud-transcribe --example deepgram_live [file.wav]`
//!
//! With a WAV (mono PCM16 at 24 kHz, e.g. from
//! `say -o x.wav --data-format=LEI16@24000 "open calculator"`) it streams the
//! file at real time, then silence for the turn to end. Without one, the mic.
//! `KEYTERMS=Aloud,Tauri` passes vocabulary the way always-listening does.

use aloud_audio::REALTIME_RATE;
use aloud_transcribe::deepgram::{self, Heard};
use std::time::Duration;
use tokio::sync::mpsc;

const CHUNK_MS: u64 = 80;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let key = std::env::var("DEEPGRAM_API_KEY")?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let _recorder = match std::env::args().nth(1) {
        Some(path) => {
            let samples = hound::WavReader::open(path)?
                .into_samples::<i16>()
                .collect::<Result<Vec<_>, _>>()?;
            tokio::spawn(async move {
                let chunk = REALTIME_RATE as usize * CHUNK_MS as usize / 1000;
                let silence = vec![0i16; REALTIME_RATE as usize * 6];
                for c in samples.chunks(chunk).chain(silence.chunks(chunk)) {
                    let _ = tx.send(c.iter().flat_map(|s| s.to_le_bytes()).collect());
                    tokio::time::sleep(Duration::from_millis(CHUNK_MS)).await;
                }
            });
            None
        }
        None => Some(aloud_audio::Recorder::tap_only(
            None,
            Box::new(move |pcm| {
                let _ = tx.send(pcm);
            }),
        )?),
    };

    let keyterms: Vec<String> = std::env::var("KEYTERMS")
        .map(|k| k.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let mut last = Heard::Partial(String::new());
    let run = deepgram::stream(&key, REALTIME_RATE, &keyterms, &mut rx, |heard| {
        if heard != last {
            println!("{heard:?}");
            last = heard;
        }
    });
    // The mic never closes the channel, so the run is bounded.
    match tokio::time::timeout(Duration::from_secs(20), run).await {
        Ok(result) => result,
        Err(_) => Ok(()),
    }
}
