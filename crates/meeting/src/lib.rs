//! Turning transcribed meeting chunks into a saved note.
//!
//! The half of meeting capture that has nothing to do with Tauri: where a
//! chunk sits on the timeline, what a gap means, and what gets written when
//! the summary fails. Recording and the OS wiring stay in the app; this is
//! what can be tested without a microphone.
//!
//! Ported from `MeetingNotesService.swift`.

use anyhow::Result;

/// Seconds of audio per chunk. Sets the timestamp granularity: a segment is
/// stamped with the start of the chunk it came from, so this is also how far
/// a quoted line can be from its real position.
pub const CHUNK_SECONDS: usize = 60;

/// Which model writes the summary, and what to fall back to.
///
/// A borrowed struct rather than the app's `Settings` so this crate does not
/// depend on the settings store — and so a test can summarise without
/// constructing a whole config.
pub struct Model<'a> {
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub model: &'a str,
    /// Tried when the primary is rate-limited. Empty is treated as absent.
    pub fallback: Option<&'a str>,
}

/// Places finished segments on the timeline by chunk index.
///
/// Chunks transcribe concurrently and finish out of order, so completion
/// order is not timeline order. A missing index stays `None` and keeps its
/// slot: closing the gap would slide every later segment earlier, and a
/// meeting's timestamps would silently drift by a minute per lost chunk.
pub fn in_order(mut done: Vec<(usize, String)>) -> Vec<Option<String>> {
    done.sort_by_key(|(index, _)| *index);
    let mut segments = vec![None; done.last().map_or(0, |(i, _)| i + 1)];
    for (index, text) in done {
        segments[index] = Some(text);
    }
    segments
}

/// Joins segments in order, stamping each with its `[hh:mm]` offset.
/// Silent chunks are skipped — they hold their slot but print nothing.
pub fn assemble(segments: &[Option<String>]) -> String {
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

/// Words captured so far, for the in-progress meeting readout.
///
/// Counted from the segment text, never from [`assemble`]'s output: the
/// `[hh:mm]` stamp it prepends is whitespace-separated, so counting the
/// assembled transcript inflates the total by one word per minute of meeting.
pub fn word_count(segments: &[Option<String>]) -> usize {
    segments
        .iter()
        .flatten()
        .map(|s| s.split_whitespace().count())
        .sum()
}

/// The note's markdown and title.
///
/// One LLM call turns the transcript into notes; **any** failure still
/// returns the raw transcript verbatim. Losing an hour of a meeting because a
/// summary request 429'd is the worst outcome this code can produce, so there
/// is no error path out of here — the caller always gets something to save.
pub async fn notes(model: Model<'_>, transcript: &str) -> (String, String) {
    if transcript.trim().is_empty() {
        return (
            "# Meeting Notes\n\n_No speech was captured._".to_string(),
            "Meeting Notes".into(),
        );
    }

    match summarize(&model, transcript).await {
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

async fn summarize(model: &Model<'_>, transcript: &str) -> Result<String> {
    let client = aloud_llm::LlmClient::new(model.api_key, Some(model.base_url))?;
    let messages = [
        aloud_llm::Message::system(aloud_llm::prompts::MEETING),
        aloud_llm::Message::user(format!(
            "TRANSCRIPT:\n<<<TRANSCRIPT\n{transcript}\nTRANSCRIPT"
        )),
    ];
    client
        .complete_with_fallback(
            model.model,
            model.fallback.filter(|f| !f.trim().is_empty()),
            &messages,
            Some(0.2),
        )
        .await
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
        assert_eq!(
            assemble(&segments),
            "[00:00] first minute\n\n[00:02] third minute"
        );
    }

    #[test]
    fn hour_boundary_stamps_correctly() {
        let mut segments = vec![None; 61];
        segments[60] = Some("past the hour".to_string());
        assert!(assemble(&segments).starts_with("[01:00]"));
    }

    /// Chunks finish out of order because they transcribe concurrently, so
    /// reassembly must key on the index. Sorting by completion order would
    /// silently shuffle a meeting's timeline.
    #[test]
    fn out_of_order_chunks_reassemble_by_position() {
        let segments = in_order(vec![
            (2, "third minute".to_string()),
            (0, "first minute".to_string()),
        ]);

        // Index 1 never arrived, so it must stay an empty slot rather than
        // letting "third minute" slide up to the 1-minute mark.
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1], None);
        assert_eq!(
            assemble(&segments),
            "[00:00] first minute\n\n[00:02] third minute"
        );
    }

    /// The regression this guards: counting `assemble()`'s output instead of
    /// the segments adds the `[hh:mm]` stamp to the total, so the live word
    /// count drifts up by one per minute — a 90-minute meeting reads 90 words
    /// longer than it is.
    #[test]
    fn the_word_count_does_not_include_the_timestamp_stamps() {
        let segments = vec![
            Some("one two three".to_string()),
            None,
            Some("four five".to_string()),
        ];
        assert_eq!(word_count(&segments), 5);
        assert_eq!(
            assemble(&segments).split_whitespace().count(),
            7,
            "the stamps are why this cannot be counted off the transcript"
        );
        assert_eq!(word_count(&[]), 0);
        assert_eq!(word_count(&[None, Some("   ".into())]), 0);
    }

    #[test]
    fn empty_session_produces_a_note_not_a_crash() {
        assert_eq!(in_order(Vec::new()), Vec::<Option<String>>::new());
        assert_eq!(assemble(&[]), "");
        assert_eq!(assemble(&[None, None]), "");
    }

    #[tokio::test]
    async fn silence_still_produces_a_saveable_note() {
        let model = Model {
            api_key: "",
            base_url: "http://127.0.0.1:1",
            model: "m",
            fallback: None,
        };
        let (markdown, title) = notes(model, "   ").await;
        assert!(markdown.contains("No speech was captured"));
        assert_eq!(title, "Meeting Notes");
    }

    /// The guarantee worth pinning: a failed summary must never cost the
    /// transcript. The base URL below refuses connections, so this exercises
    /// the real error path rather than a stub of it.
    #[tokio::test]
    async fn a_failed_summary_still_preserves_the_transcript_verbatim() {
        let model = Model {
            api_key: "k",
            base_url: "http://127.0.0.1:1",
            model: "m",
            fallback: None,
        };
        let (markdown, title) = notes(model, "[00:00] the whole meeting").await;
        assert!(
            markdown.contains("[00:00] the whole meeting"),
            "the transcript must survive a failed summary: {markdown}"
        );
        assert!(markdown.contains("## Transcript"));
        assert!(markdown.contains("Summary unavailable"));
        assert_eq!(title, "Meeting Notes");
    }
}
