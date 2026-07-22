//! Ambient meeting capture: records the mic continuously, transcribes rolling
//! ~60s chunks in the background, and on stop turns the running transcript into
//! markdown notes.
//!
//! ponytail: mic-only, as in the Swift app — remote participants are heard
//! only through the speakers the mic picks up. A system-audio tap is the
//! upgrade path.

use aloud_store::Settings;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::mpsc;

/// One rolling chunk, at the 24 kHz PCM16 the realtime tap emits.
const BYTES_PER_SECOND: usize = aloud_audio::REALTIME_RATE as usize * 2; // i16
const CHUNK_BYTES: usize = BYTES_PER_SECOND * aloud_meeting::CHUNK_SECONDS;
/// Trailing audio shorter than this on stop is dropped, not transcribed.
const MIN_FINAL_CHUNK_BYTES: usize = BYTES_PER_SECOND;

/// A running meeting-notes session.
pub struct Meeting {
    app: AppHandle,
    recorder: aloud_audio::Recorder,
    chunker: Arc<Mutex<aloud_audio::Chunker>>,
    settings: Settings,
    /// Dropping this closes the channel, which is how the worker learns the
    /// meeting is over.
    tx: Option<mpsc::UnboundedSender<(usize, Vec<u8>)>>,
    worker: tauri::async_runtime::JoinHandle<Vec<(usize, String)>>,
    started_at: Instant,
    /// Shared rather than returned: a window opening mid-meeting has to be told
    /// the total without waiting for the next chunk.
    words: Arc<AtomicU32>,
    /// Set before `stop` emits `Saving`, so the tail chunk's emit cannot land
    /// after it and re-arm the UI mid-save — see the emit itself.
    stopping: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[serde(tag = "phase", rename_all = "camelCase")]
#[tauri_specta(event_name = "meeting-phase")]
pub enum MeetingPhase {
    /// Re-emitted as each chunk finishes so the readout re-syncs instead of
    /// drifting off a single start event.
    #[serde(rename_all = "camelCase")]
    Recording {
        elapsed_secs: u32,
        words: u32,
    },
    Saving,
    Idle,
}

impl Meeting {
    pub fn phase(&self) -> MeetingPhase {
        MeetingPhase::Recording {
            elapsed_secs: self.started_at.elapsed().as_secs() as u32,
            words: self.words.load(Ordering::Relaxed),
        }
    }

    /// Starts capturing. Chunks transcribe in the background as they fill, so
    /// stopping a long meeting does not stall while an hour of audio is sent
    /// off one minute at a time.
    pub fn start(app: AppHandle, settings: Settings) -> Result<Self> {
        let chunker = Arc::new(Mutex::new(aloud_audio::Chunker::new(CHUNK_BYTES)));
        let (tx, rx) = mpsc::unbounded_channel();
        let started_at = Instant::now();
        let words = Arc::new(AtomicU32::new(0));
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker = tauri::async_runtime::spawn(transcribe_chunks(
            app.clone(),
            settings.clone(),
            rx,
            started_at,
            words.clone(),
            stopping.clone(),
        ));

        let tap_chunker = chunker.clone();
        let tap_tx = tx.clone();
        let recorder = aloud_audio::Recorder::start(
            settings.selected_microphone.as_deref(),
            Some(Box::new(move |bytes: Vec<u8>| {
                if let Ok(mut chunker) = tap_chunker.lock() {
                    // Send, never transcribe, on this thread: the tap thread
                    // must never block.
                    for chunk in chunker.push(&bytes) {
                        let _ = tap_tx.send(chunk);
                    }
                }
            })),
        )
        .context("starting meeting recorder")?;

        emit(
            &app,
            MeetingPhase::Recording {
                elapsed_secs: 0,
                words: 0,
            },
        );
        Ok(Self {
            app,
            recorder,
            chunker,
            settings,
            tx: Some(tx),
            worker,
            started_at,
            words,
            stopping,
        })
    }

    /// Stops, transcribes any tail, summarizes, and writes the note. Returns
    /// the note path.
    pub async fn stop(self) -> Result<std::path::PathBuf> {
        let Self {
            app,
            recorder,
            chunker,
            settings,
            tx,
            worker,
            stopping,
            ..
        } = self;
        // Before `Saving`, so the worker cannot race between the two.
        stopping.store(true, std::sync::atomic::Ordering::Relaxed);
        emit(&app, MeetingPhase::Saving);

        // Stop capture first, so no further chunks can be queued behind us.
        let _ = recorder.stop();

        // Closing the channel is the worker's signal to finish and return.
        let (tail_index, tail) = chunker.lock().unwrap().take_tail();
        if let Some(tx) = &tx {
            if tail.len() >= MIN_FINAL_CHUNK_BYTES {
                let _ = tx.send((tail_index, tail));
            }
        }
        drop(tx);

        let done = worker.await.unwrap_or_default();
        let transcript = aloud_meeting::assemble(&aloud_meeting::in_order(done));
        let (markdown, title) = aloud_meeting::notes(model_of(&settings), &transcript).await;

        let dir = aloud_notes::directory()?;
        let path = aloud_notes::write_note(&dir, &title, &markdown, chrono::Utc::now())?;
        emit(&app, MeetingPhase::Idle);
        Ok(path)
    }
}

fn model_of(settings: &Settings) -> aloud_meeting::Model<'_> {
    aloud_meeting::Model {
        api_key: &settings.groq_api_key,
        base_url: &settings.base_url,
        model: &settings.post_processing_model,
        fallback: Some(settings.fallback_model.as_str()),
    }
}

/// Transcribes chunks as they arrive, returning what it got, indexed. A failed
/// chunk is reported in place rather than dropped: a gap in a transcript should
/// be visible, not silently closed up.
async fn transcribe_chunks(
    app: AppHandle,
    settings: Settings,
    mut rx: mpsc::UnboundedReceiver<(usize, Vec<u8>)>,
    started_at: Instant,
    words: Arc<AtomicU32>,
    stopping: Arc<std::sync::atomic::AtomicBool>,
) -> Vec<(usize, String)> {
    let mut segments = Vec::new();
    // Built on the first chunk, not at start: the local model costs ~200ms and
    // a meeting with no audio should never pay for it.
    let mut transcriber = None;

    while let Some((index, chunk)) = rx.recv().await {
        if transcriber.is_none() {
            match crate::pipeline::build_transcriber(&app, &settings).await {
                Ok(t) => transcriber = Some(t),
                Err(e) => {
                    tracing::error!(%e, "meeting transcriber unavailable");
                    continue;
                }
            }
        }
        let Some(t) = transcriber.as_deref() else {
            continue;
        };
        match aloud_transcribe::preview::transcribe_pcm16(t, &chunk, &settings.vocabulary).await {
            Ok(text) if !text.trim().is_empty() => segments.push((index, text)),
            Ok(_) => segments.push((index, "[inaudible]".to_string())),
            Err(e) => {
                tracing::warn!(%e, "meeting chunk transcription failed");
                segments.push((index, "[inaudible]".to_string()));
            }
        }

        // Summed incrementally rather than recomputed off the assembled
        // transcript, which cloned every segment on every chunk.
        let counted = words.fetch_add(
            segments
                .last()
                .map_or(0, |(_, text)| text.split_whitespace().count() as u32),
            Ordering::Relaxed,
        ) + segments
            .last()
            .map_or(0, |(_, text)| text.split_whitespace().count() as u32);

        // `stop` emits `Saving` and *then* sends the tail chunk, so without
        // this the UI flips back to "recording" for the whole summarisation and
        // its "Stop and Save" button opens a brand-new meeting.
        if stopping.load(Ordering::Relaxed) {
            continue;
        }
        emit(
            &app,
            MeetingPhase::Recording {
                elapsed_secs: started_at.elapsed().as_secs() as u32,
                words: counted,
            },
        );
    }
    segments
}

fn emit(app: &AppHandle, phase: MeetingPhase) {
    if let Err(e) = phase.emit(app) {
        tracing::warn!(%e, "could not emit meeting phase");
    }
}
