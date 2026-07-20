//! Ambient meeting capture, ported from `MeetingNotesService.swift`.
//!
//! Records the mic continuously, transcribes rolling ~60s chunks in the
//! background, and on stop turns the running transcript into structured
//! markdown notes saved to the notes directory.
//!
//! ponytail: mic-only, as in the Swift app — remote participants are heard
//! only through the speakers the mic picks up. A system-audio tap is the
//! upgrade path.

use aloud_store::Settings;
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};
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
    /// Chunks go to the worker as they fill. Dropping this closes the channel,
    /// which is how the worker learns the meeting is over.
    tx: Option<mpsc::UnboundedSender<(usize, Vec<u8>)>>,
    /// The background transcriber, joined on stop.
    worker: tauri::async_runtime::JoinHandle<Vec<(usize, String)>>,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "meeting-phase")]
pub enum MeetingPhase {
    Recording,
    Saving,
    Idle,
}

impl Meeting {
    /// Starts capturing. Chunks transcribe in the background as they fill, so
    /// stopping a long meeting does not stall while an hour of audio is sent
    /// off one minute at a time.
    pub fn start(app: AppHandle, settings: Settings) -> Result<Self> {
        let chunker = Arc::new(Mutex::new(aloud_audio::Chunker::new(CHUNK_BYTES)));
        let (tx, rx) = mpsc::unbounded_channel();
        let worker =
            tauri::async_runtime::spawn(transcribe_chunks(app.clone(), settings.clone(), rx));

        // The realtime tap hands us 24 kHz PCM16 as it arrives; accumulate it
        // and hand off a chunk every 60 seconds.
        let tap_chunker = chunker.clone();
        let tap_tx = tx.clone();
        let recorder = aloud_audio::Recorder::start(
            settings.selected_microphone.as_deref(),
            Some(Box::new(move |bytes: Vec<u8>| {
                if let Ok(mut chunker) = tap_chunker.lock() {
                    // Send, never transcribe, on this thread: the tap thread
                    // must never block. An unbounded channel cannot back up.
                    for chunk in chunker.push(&bytes) {
                        let _ = tap_tx.send(chunk);
                    }
                }
            })),
        )
        .context("starting meeting recorder")?;

        emit(&app, MeetingPhase::Recording);
        Ok(Self {
            app,
            recorder,
            chunker,
            settings,
            tx: Some(tx),
            worker,
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
        } = self;
        emit(&app, MeetingPhase::Saving);

        // Stop capture first, so no further chunks can be queued behind us.
        let _ = recorder.stop();

        // Send the trailing audio as a final chunk if it is long enough to be
        // worth transcribing, then close the channel: that is the worker's
        // signal to finish what it has and return.
        let (tail_index, tail) = chunker.lock().unwrap().take_tail();
        if let Some(tx) = &tx {
            if tail.len() >= MIN_FINAL_CHUNK_BYTES {
                let _ = tx.send((tail_index, tail));
            }
        }
        drop(tx);

        // Most chunks are already transcribed; this waits only on whatever was
        // still in flight, rather than on the whole meeting.
        let done = worker.await.unwrap_or_default();
        let transcript = aloud_meeting::assemble(&aloud_meeting::in_order(done));
        let (markdown, title) = aloud_meeting::notes(model_of(&settings), &transcript).await;

        let dir = aloud_notes::directory()?;
        let path = aloud_notes::write_note(&dir, &title, &markdown, chrono::Utc::now())?;
        emit(&app, MeetingPhase::Idle);
        Ok(path)
    }
}

/// The model settings the notes summary runs against.
fn model_of(settings: &Settings) -> aloud_meeting::Model<'_> {
    aloud_meeting::Model {
        api_key: &settings.groq_api_key,
        base_url: &settings.base_url,
        model: &settings.post_processing_model,
        fallback: Some(settings.fallback_model.as_str()),
    }
}

/// Transcribes chunks as they arrive, returning what it got, indexed.
///
/// Runs for the life of the meeting and ends when the channel closes. A failed
/// chunk is reported in place rather than dropped: a gap in the middle of a
/// transcript should be visible, not silently closed up.
async fn transcribe_chunks(
    app: AppHandle,
    settings: Settings,
    mut rx: mpsc::UnboundedReceiver<(usize, Vec<u8>)>,
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
        match transcribe_chunk(t, &chunk, index).await {
            Some(text) => segments.push((index, text)),
            None => segments.push((index, "[inaudible]".to_string())),
        }
    }
    segments
}

/// Transcribes one PCM16 chunk by wrapping it in a WAV the transcriber reads.
async fn transcribe_chunk(
    transcriber: &dyn aloud_transcribe::Transcriber,
    pcm16: &[u8],
    index: usize,
) -> Option<String> {
    let wav = std::env::temp_dir().join(format!("aloud-meeting-{index}.wav"));
    if let Err(e) = aloud_audio::write_pcm16_wav(&wav, pcm16) {
        tracing::warn!(%e, "could not stage meeting chunk");
        return None;
    }
    let result = transcriber.transcribe(&wav).await;
    let _ = std::fs::remove_file(&wav);
    match result {
        Ok(text) if !text.trim().is_empty() => Some(text),
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(%e, "meeting chunk transcription failed");
            None
        }
    }
}

fn emit(app: &AppHandle, phase: MeetingPhase) {
    if let Err(e) = phase.emit(app) {
        tracing::warn!(%e, "could not emit meeting phase");
    }
}
