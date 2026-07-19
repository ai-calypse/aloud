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
const CHUNK_SECONDS: usize = 60;
const SAMPLE_RATE: usize = aloud_audio::REALTIME_RATE as usize;
const BYTES_PER_SECOND: usize = SAMPLE_RATE * 2; // i16
const CHUNK_BYTES: usize = BYTES_PER_SECOND * CHUNK_SECONDS;
/// Trailing audio shorter than this on stop is dropped, not transcribed.
const MIN_FINAL_CHUNK_BYTES: usize = BYTES_PER_SECOND;

/// A running meeting-notes session.
pub struct Meeting {
    app: AppHandle,
    recorder: aloud_audio::Recorder,
    state: Arc<Mutex<State>>,
    settings: Settings,
    /// Chunks go to the worker as they fill. Dropping this closes the channel,
    /// which is how the worker learns the meeting is over.
    tx: Option<mpsc::UnboundedSender<(usize, Vec<u8>)>>,
    /// The background transcriber, joined on stop.
    worker: tauri::async_runtime::JoinHandle<Vec<(usize, String)>>,
}

#[derive(Default)]
struct State {
    /// Audio not yet cut into a chunk.
    pending: Vec<u8>,
    /// Position of the next chunk, so segments can be reassembled in order
    /// even though they finish out of order.
    next_index: usize,
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
        let state = Arc::new(Mutex::new(State::default()));
        let (tx, rx) = mpsc::unbounded_channel();
        let worker =
            tauri::async_runtime::spawn(transcribe_chunks(app.clone(), settings.clone(), rx));

        // The realtime tap hands us 24 kHz PCM16 as it arrives; accumulate it
        // and hand off a chunk every 60 seconds.
        let tap_state = state.clone();
        let tap_tx = tx.clone();
        let recorder = aloud_audio::Recorder::start(
            settings.selected_microphone.as_deref(),
            Some(Box::new(move |bytes: Vec<u8>| {
                if let Ok(mut state) = tap_state.lock() {
                    state.pending.extend_from_slice(&bytes);
                    // Send, never transcribe, on this thread: the tap thread
                    // must never block. An unbounded channel cannot back up.
                    while state.pending.len() >= CHUNK_BYTES {
                        let chunk: Vec<u8> = state.pending.drain(..CHUNK_BYTES).collect();
                        let index = state.next_index;
                        state.next_index += 1;
                        let _ = tap_tx.send((index, chunk));
                    }
                }
            })),
        )
        .context("starting meeting recorder")?;

        emit(&app, MeetingPhase::Recording);
        Ok(Self {
            app,
            recorder,
            state,
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
            state,
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
        let (tail, tail_index) = {
            let mut state = state.lock().unwrap();
            (std::mem::take(&mut state.pending), state.next_index)
        };
        if let Some(tx) = &tx {
            if tail.len() >= MIN_FINAL_CHUNK_BYTES {
                let _ = tx.send((tail_index, tail));
            }
        }
        drop(tx);

        // Most chunks are already transcribed; this waits only on whatever was
        // still in flight, rather than on the whole meeting.
        let mut done = worker.await.unwrap_or_default();
        done.sort_by_key(|(index, _)| *index);

        // Reassemble by position: a dropped or silent chunk must still occupy
        // its slot, or every later timestamp shifts.
        let mut segments = vec![None; done.last().map_or(0, |(i, _)| i + 1)];
        for (index, text) in done {
            segments[index] = Some(text);
        }

        let transcript = assemble(&segments);
        let (markdown, title) = make_notes(&settings, &transcript).await;

        let dir = aloud_notes::directory()?;
        let path = aloud_notes::write_note(&dir, &title, &markdown, chrono::Utc::now())?;
        emit(&app, MeetingPhase::Idle);
        Ok(path)
    }
}

/// One LLM call turns the transcript into notes; any failure preserves the
/// raw transcript verbatim so a meeting is never lost.
async fn make_notes(settings: &Settings, transcript: &str) -> (String, String) {
    if transcript.trim().is_empty() {
        let md = "# Meeting Notes\n\n_No speech was captured._".to_string();
        return (md, "Meeting Notes".into());
    }

    match summarize(settings, transcript).await {
        Ok(mut notes) => {
            // Never drop the transcript, even if the model omitted it.
            if !notes.contains("## Transcript") {
                notes.push_str(&format!("\n\n## Transcript\n\n{transcript}"));
            }
            let title =
                aloud_notes::title_from_markdown(&notes).unwrap_or_else(|| "Meeting Notes".into());
            (notes, title)
        }
        Err(e) => {
            tracing::error!(%e, "meeting summary failed; saving raw transcript");
            let md = format!(
                "# Meeting Notes\n\n_Summary unavailable — {e}_\n\n## Transcript\n\n{transcript}"
            );
            (md, "Meeting Notes".into())
        }
    }
}

async fn summarize(settings: &Settings, transcript: &str) -> Result<String> {
    let client = aloud_llm::LlmClient::new(&settings.groq_api_key, Some(&settings.base_url))?;
    let messages = [
        aloud_llm::Message::system(aloud_llm::prompts::MEETING),
        aloud_llm::Message::user(format!(
            "TRANSCRIPT:\n<<<TRANSCRIPT\n{transcript}\nTRANSCRIPT"
        )),
    ];
    client
        .complete_with_fallback(
            &settings.post_processing_model,
            Some(settings.fallback_model.as_str()).filter(|f| !f.trim().is_empty()),
            &messages,
            Some(0.2),
        )
        .await
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
    if let Err(e) = write_pcm16_wav(&wav, pcm16) {
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

/// Writes the tap's raw 24 kHz mono PCM16 bytes as a 16 kHz WAV — the
/// transcriber contract is 16 kHz mono (local whisper hard-rejects anything
/// else; the cloud API merely tolerated 24 kHz).
fn write_pcm16_wav(path: &std::path::Path, pcm16: &[u8]) -> Result<()> {
    let samples: Vec<f32> = pcm16
        .chunks_exact(2)
        .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / f32::from(i16::MAX))
        .collect();
    let samples = aloud_audio::resample::resample_mono(
        &samples,
        aloud_audio::REALTIME_RATE,
        aloud_audio::TRANSCRIBE_RATE,
    )?;
    aloud_audio::write_wav(path, &samples, aloud_audio::TRANSCRIBE_RATE)
}

/// Joins finished segments in order, stamping each with its `[hh:mm]` offset.
/// Silent chunks (no transcript) are skipped.
fn assemble(segments: &[Option<String>]) -> String {
    segments
        .iter()
        .enumerate()
        .filter_map(|(i, seg)| {
            let text = seg.as_ref()?;
            let start = i * CHUNK_SECONDS;
            Some(format!(
                "[{:02}:{:02}] {text}",
                start / 3600,
                (start % 3600) / 60
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn emit(app: &AppHandle, phase: MeetingPhase) {
    if let Err(e) = phase.emit(app) {
        tracing::warn!(%e, "could not emit meeting phase");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_timestamped_transcript_skipping_silence() {
        let segments = vec![
            Some("first minute".to_string()),
            None, // a silent chunk
            Some("third minute".to_string()),
        ];
        let text = assemble(&segments);
        assert_eq!(text, "[00:00] first minute\n\n[00:02] third minute");
    }

    #[test]
    fn hour_boundary_stamps_correctly() {
        let mut segments = vec![None; 61];
        segments[60] = Some("past the hour".to_string());
        assert!(assemble(&segments).starts_with("[01:00]"));
    }

    /// Chunks now finish out of order (they transcribe concurrently), so the
    /// reassembly must key on the chunk's index. Sorting by completion order
    /// would silently shuffle a meeting's timeline.
    #[test]
    fn out_of_order_chunks_reassemble_by_position() {
        let mut done = vec![
            (2, "third minute".to_string()),
            (0, "first minute".to_string()),
        ];
        done.sort_by_key(|(index, _)| *index);

        let mut segments = vec![None; done.last().map_or(0, |(i, _)| i + 1)];
        for (index, text) in done {
            segments[index] = Some(text);
        }

        // Index 1 never arrived, so it must stay an empty slot rather than
        // letting "third minute" slide up to the 1-minute mark.
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1], None);
        assert_eq!(
            assemble(&segments),
            "[00:00] first minute\n\n[00:02] third minute"
        );
    }

    #[test]
    fn empty_session_produces_a_note_not_a_crash() {
        assert_eq!(assemble(&[]), "");
        assert_eq!(assemble(&[None, None]), "");
    }

    /// The staged chunk WAV must be 16 kHz mono — local whisper hard-rejects
    /// the tap's native 24 kHz, which silently dropped every meeting chunk.
    #[test]
    fn chunk_wav_is_16k_mono_and_not_silent() {
        let samples: Vec<f32> = (0..SAMPLE_RATE) // 1s of 440 Hz at 24 kHz
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / SAMPLE_RATE as f32).sin())
            .collect();
        let pcm16 = aloud_audio::resample::to_pcm16_bytes(&samples);

        let path = std::env::temp_dir().join("aloud-test-meeting-chunk.wav");
        write_pcm16_wav(&path, &pcm16).unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels), (16_000, 1));
        let peak = reader
            .samples::<i16>()
            .map(|s| s.unwrap().unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 8000, "resampled audio must not be near-silence");
        let _ = std::fs::remove_file(&path);
    }
}
