//! Showing the user what they are saying, while they are saying it.
//!
//! **Display only** — nothing here reaches the clipboard, so it can be fast and
//! loose in a way the real transcript cannot.
//!
//! Re-transcribing, not chunking: an open window is transcribed again from the
//! top every pass, so a half-spoken word is whole at the next one. Past
//! [`COMMIT_SECONDS`] the window's text is frozen and its audio dropped, which
//! bounds what a pass costs however long the user talks.
//!
//! ponytail: a fixed tick and a fixed window. Cutting the window on a pause
//! (the energy floor `aloud_audio::loudness` already computes) would put the
//! freeze on a word boundary instead of on the clock — the upgrade path if
//! commits are seen to chop words.

use crate::Transcriber;
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Distinguishes concurrent stagings: a meeting and a dictation stage WAVs at
/// the same time, and a shared name is one writing what another reads.
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// Transcribes one buffer of 24 kHz mono PCM16 — the rate the realtime tap
/// emits — by staging it as the 16 kHz WAV both engines want. The file is
/// removed whether or not transcription succeeded.
pub async fn transcribe_pcm16(transcriber: &dyn Transcriber, pcm16: &[u8]) -> Result<String> {
    let path = std::env::temp_dir().join(format!(
        "aloud-chunk-{}-{}.wav",
        std::process::id(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    ));
    aloud_audio::write_pcm16_wav(&path, pcm16)?;
    let result = transcriber.transcribe(&path).await;
    let _ = std::fs::remove_file(&path);
    result
}

const BYTES_PER_SECOND: usize = aloud_audio::REALTIME_RATE as usize * 2; // i16

/// How long the open window may grow before its text is frozen. Every pass
/// re-transcribes it, so raising this puts the preview behind the speaker.
pub const COMMIT_SECONDS: usize = 8;
const COMMIT_BYTES: usize = BYTES_PER_SECOND * COMMIT_SECONDS;

/// Whisper answers a fragment shorter than this with invented punctuation or
/// nothing at all; waiting a tick costs the user nothing.
const MIN_BYTES: usize = BYTES_PER_SECOND / 2;

/// Gap between passes. Plus the pass itself (~200-400ms for `base.en` on 8s
/// with Metal) that lands the preview about half a second behind the voice.
const TICK: Duration = Duration::from_millis(250);

/// A running live preview. Feed it with [`Preview::tap`]; drop it to stop.
/// There is no `finish` — dropping aborts the worker mid-pass, which is the
/// point once the real transcription is the only one worth CPU.
pub struct Preview {
    /// Audio captured but not yet frozen into `committed` by the worker.
    audio: Arc<Mutex<Vec<u8>>>,
    worker: Option<tokio::task::JoinHandle<()>>,
}

impl Preview {
    /// Starts the background previewer.
    ///
    /// `transcriber` is a future because building one loads a model, which must
    /// not block the thread that opens the microphone; it is awaited on the
    /// first pass, so a silent capture never pays for it.
    ///
    /// `runtime` is passed in because a bare `tokio::spawn` **panics with no
    /// runtime on the calling thread**, and a dictation starts on the CGEventTap
    /// callback thread.
    pub fn spawn(
        runtime: &tokio::runtime::Handle,
        transcriber: impl std::future::Future<Output = Result<Arc<dyn Transcriber>>> + Send + 'static,
        on_partial: Box<dyn Fn(&str) + Send>,
    ) -> Self {
        let audio = Arc::new(Mutex::new(Vec::new()));
        Self {
            audio: audio.clone(),
            worker: Some(runtime.spawn(run(audio, transcriber, on_partial))),
        }
    }

    /// The callback to hand `aloud_audio::Recorder::start`. Runs on the audio
    /// callback thread, so it only appends — work here stalls capture.
    pub fn tap(&self) -> Box<dyn FnMut(Vec<u8>) + Send> {
        let audio = self.audio.clone();
        Box::new(move |bytes: Vec<u8>| {
            if let Ok(mut buf) = audio.lock() {
                buf.extend_from_slice(&bytes);
            }
        })
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

/// Re-transcribes the open window until the preview is dropped.
async fn run(
    audio: Arc<Mutex<Vec<u8>>>,
    transcriber: impl std::future::Future<Output = Result<Arc<dyn Transcriber>>>,
    on_partial: Box<dyn Fn(&str) + Send>,
) {
    let mut committed = String::new();
    // The open window's latest hypothesis, kept across passes.
    let mut tail = String::new();
    let mut built: Option<Arc<dyn Transcriber>> = None;
    let mut build = Some(transcriber);

    loop {
        tokio::time::sleep(TICK).await;

        let Some(window) = audio
            .lock()
            .ok()
            .filter(|buf| buf.len() >= MIN_BYTES)
            .map(|buf| buf.clone())
        else {
            continue;
        };

        if built.is_none() {
            match build.take().expect("build is taken only on success").await {
                Ok(t) => built = Some(t),
                // No model, no preview; the dictation itself is unaffected.
                Err(e) => {
                    tracing::warn!(%e, "no transcriber for the live preview");
                    return;
                }
            }
        }

        let text = match transcribe_pcm16(built.as_deref().expect("built above"), &window).await {
            Ok(text) => text,
            // One failure means every pass fails the same way; stop rather than
            // log four times a second for the rest of the dictation.
            Err(e) => {
                tracing::warn!(%e, "live preview stopped");
                return;
            }
        };

        // Whisper answers an uncertain window with nothing, and the
        // hallucination filter blanks others; replacing the tail with that makes
        // already-read words flicker out and back a tick later.
        let text = text.trim();
        if !text.is_empty() {
            tail = text.to_string();
        }

        let mut shown = committed.clone();
        if !tail.is_empty() {
            if !shown.is_empty() {
                shown.push(' ');
            }
            shown.push_str(&tail);
        }
        on_partial(&shown);

        // Window full: freeze its text and drain exactly the bytes transcribed,
        // so whatever the tap appended mid-pass stays for the next one.
        if window.len() >= COMMIT_BYTES {
            committed = shown;
            tail.clear();
            if let Ok(mut buf) = audio.lock() {
                buf.drain(..window.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;

    struct Fake {
        /// One reply per call, in order.
        replies: Mutex<std::collections::VecDeque<Result<String>>>,
    }

    #[async_trait]
    impl Transcriber for Fake {
        async fn transcribe(&self, _: &Path) -> Result<String> {
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn fake(replies: Vec<Result<String>>) -> Arc<dyn Transcriber> {
        Arc::new(Fake {
            replies: Mutex::new(replies.into()),
        })
    }

    /// Real speech at 24 kHz, so `write_pcm16_wav` has something to resample.
    fn tone(seconds: f32) -> Vec<u8> {
        let n = (aloud_audio::REALTIME_RATE as f32 * seconds) as usize;
        let samples: Vec<f32> = (0..n).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        aloud_audio::resample::to_pcm16_bytes(&samples)
    }

    /// Guards: a dictation longer than the window showed only its last seconds,
    /// so text vanished upwards mid-sentence.
    #[tokio::test]
    async fn text_from_before_the_window_slid_stays_on_screen() {
        let partials: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = partials.clone();
        let t = fake(vec![Ok("hello there".into()), Ok("and goodbye".into())]);
        let preview = Preview::spawn(
            &tokio::runtime::Handle::current(),
            async move { Ok(t) },
            Box::new(move |p| seen.lock().unwrap().push(p.to_string())),
        );
        let mut tap = preview.tap();

        // A full window: the first pass transcribes it and commits.
        tap(tone(COMMIT_SECONDS as f32));
        tokio::time::sleep(Duration::from_millis(700)).await;
        // A second of new speech, which is all the window holds now.
        tap(tone(1.0));
        tokio::time::sleep(Duration::from_millis(700)).await;

        assert_eq!(
            partials.lock().unwrap().last().map(String::as_str),
            Some("hello there and goodbye")
        );
    }

    /// Guards: an empty pass used to wipe the tail, so a word appeared and
    /// vanished a tick later.
    #[tokio::test]
    async fn a_pass_that_comes_back_empty_leaves_the_words_up() {
        let partials: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = partials.clone();
        let t = fake(vec![Ok("half a sentence".into()), Ok(String::new())]);
        let preview = Preview::spawn(
            &tokio::runtime::Handle::current(),
            async move { Ok(t) },
            Box::new(move |p| seen.lock().unwrap().push(p.to_string())),
        );
        let mut tap = preview.tap();

        tap(tone(1.0));
        tokio::time::sleep(Duration::from_millis(900)).await;

        assert_eq!(
            partials.lock().unwrap().last().map(String::as_str),
            Some("half a sentence")
        );
    }
}
