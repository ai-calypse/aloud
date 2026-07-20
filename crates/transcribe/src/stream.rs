//! Transcribing audio *while* it is captured, instead of once at the end.
//!
//! Both engines take a file, so the unit of streaming here is a chunk of PCM16
//! staged as a WAV — not a socket. That is the whole trick: a chunk and a
//! whole recording are the same thing to a [`Transcriber`], so streaming needs
//! no second implementation of either engine.
//!
//! **This trades accuracy for latency, and the trade is real.** Whisper is
//! given ~5 seconds at a time with no context from the chunk before it, and
//! the cut lands wherever the clock says rather than on a pause, so a word
//! straddling a boundary can come back mangled from both sides. A single pass
//! over the whole utterance is still the better transcript. That is why the
//! setting gating this defaults to off.
//!
//! ponytail: fixed-duration cuts, tuned by `CHUNK_SECONDS` alone. Splitting on
//! silence (a VAD, or the energy floor `aloud_audio::loudness` already
//! computes) is the upgrade path, and would remove the boundary problem rather
//! than trading it.

use crate::Transcriber;
use aloud_audio::Chunker;
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Distinguishes concurrent stagings. A meeting and a dictation stream at the
/// same time, and chunks within one stream can overlap, so a name keyed on
/// anything shared is one process writing a WAV while another reads it.
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// Transcribes one buffer of 24 kHz mono PCM16 — the rate the realtime tap
/// emits — by staging it as the 16 kHz WAV both engines want.
///
/// The file is removed whether or not transcription succeeded.
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

/// How much audio goes in one chunk.
///
/// Short enough that an ordinary dictation completes at least one chunk before
/// the key is released — a 10-second chunk never fires on a 6-second utterance
/// and buys nothing — and long enough that whisper has a sentence to work
/// with. Every boundary is a place the transcript can go wrong, so this is the
/// number to raise if quality is the complaint and lower if latency is.
pub const CHUNK_SECONDS: usize = 5;

const BYTES_PER_SECOND: usize = aloud_audio::REALTIME_RATE as usize * 2; // i16
pub const CHUNK_BYTES: usize = BYTES_PER_SECOND * CHUNK_SECONDS;

/// Trailing audio shorter than this is dropped rather than transcribed.
///
/// Far smaller than the meeting path's one second: the tail of a *dictation*
/// is the end of the sentence, and dropping a quarter of a second of it loses
/// the last word. Only true fragments — the few milliseconds between the last
/// syllable and the key coming up — fall under it.
const MIN_TAIL_BYTES: usize = BYTES_PER_SECOND / 4;

/// A live transcription of a PCM16 stream.
///
/// Feed it with [`Stream::tap`] and collect the transcript with
/// [`Stream::finish`]. Chunks transcribe on a background task as they fill, so
/// by the time the stream ends only the tail is still outstanding.
pub struct Stream {
    chunker: Arc<Mutex<Chunker>>,
    /// Kept so `finish` can send the tail before closing the channel; closing
    /// it is how the worker learns the stream is over.
    tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    /// `None` only after `finish` or `drop` has taken it.
    worker: Option<tokio::task::JoinHandle<Option<Vec<String>>>>,
}

impl Stream {
    /// Starts the background transcriber.
    ///
    /// `transcriber` is a future rather than a value because building one can
    /// download and load a model, which must not happen on the thread that
    /// opens the microphone. It is awaited on the first chunk, not at spawn,
    /// so a capture with no audio in it never pays for the model.
    ///
    /// `on_partial` is called with the transcript so far after every chunk.
    /// It is feedback only — see [`Stream::finish`] for why partial text must
    /// never be pasted.
    ///
    /// `runtime` is passed in rather than found with a bare `tokio::spawn`,
    /// which **panics when there is no runtime on the calling thread**. A
    /// dictation starts on the OS key-event thread — a CGEventTap callback,
    /// which is not a tokio worker — so the bare form would take the whole app
    /// down on the first press with this turned on.
    pub fn spawn(
        runtime: &tokio::runtime::Handle,
        transcriber: impl std::future::Future<Output = Result<Arc<dyn Transcriber>>> + Send + 'static,
        on_partial: Box<dyn Fn(&str) + Send>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            chunker: Arc::new(Mutex::new(Chunker::new(CHUNK_BYTES))),
            tx: Some(tx),
            worker: Some(runtime.spawn(run(rx, transcriber, on_partial))),
        }
    }

    /// The callback to hand `aloud_audio::Recorder::start`.
    ///
    /// Runs on the audio callback thread, so it only buffers and sends: an
    /// unbounded channel cannot back up, and transcribing here would stall
    /// capture itself.
    pub fn tap(&self) -> Box<dyn FnMut(Vec<u8>) + Send> {
        let chunker = self.chunker.clone();
        let tx = self.tx.clone();
        Box::new(move |bytes: Vec<u8>| {
            let (Ok(mut chunker), Some(tx)) = (chunker.lock(), tx.as_ref()) else {
                return;
            };
            // The chunk index is dropped on purpose. It exists because meeting
            // chunks are reassembled out of order; here the worker drains one
            // channel in order, so the order is already the audio's.
            for (_, chunk) in chunker.push(&bytes) {
                let _ = tx.send(chunk);
            }
        })
    }

    /// Sends the trailing audio and waits for the finished transcript.
    ///
    /// `None` means **do not trust this stream** — the transcriber could not
    /// be built, or a chunk failed. A half-transcribed utterance is worse than
    /// none, because it looks like a complete one: the caller's job on `None`
    /// is to fall back to transcribing the whole recording, which is the path
    /// that was always there.
    ///
    /// This still returns text to *paste at the end*, deliberately. Partial
    /// text is never typed into the user's document as it arrives: whisper
    /// revises — a later chunk changes how an earlier one is punctuated,
    /// capitalised, or spelled — and there is no way to un-type what has
    /// already gone into someone's editor. The win is that most of the audio
    /// is already transcribed when the key comes up, so the wait after release
    /// is the tail, not the utterance.
    pub async fn finish(mut self) -> Option<String> {
        let tail = self.chunker.lock().ok().map(|mut c| c.take_tail().1);
        match (self.tx.take(), tail) {
            // Sent before the sender is dropped, or the worker would see the
            // channel close with the end of the sentence still in it.
            (Some(tx), Some(tail)) if tail.len() >= MIN_TAIL_BYTES => {
                let _ = tx.send(tail);
            }
            _ => {}
        }

        Some(self.worker.take()?.await.ok().flatten()?.join(" "))
    }
}

impl Drop for Stream {
    /// A dropped stream is an abandoned capture — a cancel, or a failure
    /// before `finish`. Aborting the worker is what actually stops an upload
    /// already in flight, the same reasoning the one-shot path relies on when
    /// it drops the transcribe future.
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

/// Transcribes chunks as they arrive, ending when the channel closes.
///
/// Returns `None` the moment anything fails, rather than a shorter transcript:
/// see [`Stream::finish`].
async fn run(
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
    transcriber: impl std::future::Future<Output = Result<Arc<dyn Transcriber>>>,
    on_partial: Box<dyn Fn(&str) + Send>,
) -> Option<Vec<String>> {
    let mut segments: Vec<String> = Vec::new();
    let mut built = None;
    let mut build = Some(transcriber);

    while let Some(chunk) = rx.recv().await {
        if built.is_none() {
            match build.take()?.await {
                Ok(t) => built = Some(t),
                Err(e) => {
                    tracing::warn!(%e, "streaming transcriber unavailable");
                    return None;
                }
            }
        }
        match transcribe_pcm16(built.as_deref()?, &chunk).await {
            // A chunk of silence is a normal chunk, not a failure — but it is
            // dropped rather than joined, or every pause in a dictation would
            // widen into a run of spaces in the pasted text.
            Ok(text) if text.trim().is_empty() => continue,
            Ok(text) => segments.push(text.trim().to_string()),
            Err(e) => {
                tracing::warn!(%e, "streaming chunk failed");
                return None;
            }
        }
        on_partial(&segments.join(" "));
    }
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;

    struct Fake {
        /// One reply per call, in order. An `Err` stands for a failed chunk.
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
        let samples: Vec<f32> = (0..n)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .collect::<Vec<_>>();
        aloud_audio::resample::to_pcm16_bytes(&samples)
    }

    /// The whole point of the stream: audio pushed during capture is already
    /// transcribed when it ends, and the tail is not lost. A tail dropped here
    /// silently eats the last word of every dictation.
    #[tokio::test]
    async fn chunks_and_the_tail_all_reach_the_transcript() {
        let t = fake(vec![Ok("hello there".into()), Ok(" and goodbye".into())]);
        let stream = Stream::spawn(
            &tokio::runtime::Handle::current(),
            async move { Ok(t) },
            Box::new(|_| {}),
        );
        let mut tap = stream.tap();
        // One whole chunk plus a tail worth keeping.
        tap(tone(CHUNK_SECONDS as f32 + 1.0));
        drop(tap);

        assert_eq!(
            stream.finish().await.as_deref(),
            Some("hello there and goodbye")
        );
    }

    /// A failed chunk must poison the whole stream. Returning what it managed
    /// would paste a truncated utterance that looks complete, and the caller
    /// would have no way to know to re-transcribe.
    #[tokio::test]
    async fn one_failed_chunk_discards_the_stream() {
        let t = fake(vec![Err(anyhow::anyhow!("429")), Ok("never seen".into())]);
        let stream = Stream::spawn(
            &tokio::runtime::Handle::current(),
            async move { Ok(t) },
            Box::new(|_| {}),
        );
        let mut tap = stream.tap();
        tap(tone(CHUNK_SECONDS as f32 + 1.0));
        drop(tap);

        assert_eq!(stream.finish().await, None);
    }

    /// A capture too short to fill a chunk still has to transcribe — this is
    /// most dictations.
    #[tokio::test]
    async fn a_capture_shorter_than_one_chunk_is_all_tail() {
        let t = fake(vec![Ok("quick one".into())]);
        let stream = Stream::spawn(
            &tokio::runtime::Handle::current(),
            async move { Ok(t) },
            Box::new(|_| {}),
        );
        let mut tap = stream.tap();
        tap(tone(1.0));
        drop(tap);

        assert_eq!(stream.finish().await.as_deref(), Some("quick one"));
    }
}
