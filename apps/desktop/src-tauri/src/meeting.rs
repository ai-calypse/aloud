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
use tauri::{AppHandle, Emitter};

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
}

#[derive(Default)]
struct State {
    /// Audio not yet cut into a chunk.
    pending: Vec<u8>,
    /// Full 60s chunks awaiting transcription. Staged on the tap thread (which
    /// must never block) and drained on stop.
    pending_chunks: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MeetingPhase {
    Recording,
    Saving,
    Idle,
}

impl Meeting {
    /// Starts capturing. Chunks transcribe in the background as they fill.
    pub fn start(app: AppHandle, settings: Settings) -> Result<Self> {
        let state = Arc::new(Mutex::new(State::default()));

        // The realtime tap hands us 24 kHz PCM16 as it arrives; accumulate it
        // and cut a chunk every 60 seconds.
        let tap_state = state.clone();
        let recorder = aloud_audio::Recorder::start(
            settings.selected_microphone.as_deref(),
            Some(Box::new(move |bytes: Vec<u8>| {
                if let Ok(mut state) = tap_state.lock() {
                    state.pending.extend_from_slice(&bytes);
                    // Stage full chunks only; transcription happens on stop.
                    // The tap thread must never block.
                    while state.pending.len() >= CHUNK_BYTES {
                        let chunk = state.pending.drain(..CHUNK_BYTES).collect();
                        state.pending_chunks.push(chunk);
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
        } = self;
        emit(&app, MeetingPhase::Saving);

        // Flush remaining audio into a final chunk if it is long enough to be
        // worth transcribing.
        let (chunks, tail) = {
            let mut state = state.lock().unwrap();
            let chunks = std::mem::take(&mut state.pending_chunks);
            let tail = std::mem::take(&mut state.pending);
            (chunks, tail)
        };
        let _ = recorder.stop();

        let transcriber = crate::pipeline::build_transcriber(&settings).await?;

        // Transcribe every staged chunk in order, plus the tail. Full chunks
        // are all CHUNK_BYTES, so index the temp file by position, not length.
        let mut segments = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            segments.push(transcribe_chunk(&*transcriber, chunk, i).await);
        }
        if tail.len() >= MIN_FINAL_CHUNK_BYTES {
            segments.push(transcribe_chunk(&*transcriber, &tail, segments.len()).await);
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

/// Writes raw 24 kHz mono PCM16 bytes as a WAV.
fn write_pcm16_wav(path: &std::path::Path, pcm16: &[u8]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: aloud_audio::REALTIME_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for pair in pcm16.chunks_exact(2) {
        writer.write_sample(i16::from_le_bytes([pair[0], pair[1]]))?;
    }
    writer.finalize()?;
    Ok(())
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
    if let Err(e) = app.emit("meeting-phase", phase) {
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

    #[test]
    fn empty_session_produces_a_note_not_a_crash() {
        assert_eq!(assemble(&[]), "");
        assert_eq!(assemble(&[None, None]), "");
    }
}
