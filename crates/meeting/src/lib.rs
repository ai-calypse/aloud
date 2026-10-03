//! Turning transcribed meeting chunks into a saved note: where a chunk sits on
//! the timeline, what a gap means, and what gets written when the summary
//! fails. Recording and OS wiring stay in the app.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Seconds of audio per chunk, and so the timestamp granularity: a segment is
/// stamped with its chunk's start, not its own.
pub const CHUNK_SECONDS: usize = 60;

/// Which model writes the summary, and what to fall back to. Borrowed rather
/// than the app's `Settings` so this crate does not depend on the store.
pub struct Model<'a> {
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub model: &'a str,
    /// Tried when the primary is rate-limited. Empty is treated as absent.
    pub fallback: Option<&'a str>,
}

/// Places finished segments on the timeline by chunk index — chunks transcribe
/// concurrently, so completion order is not timeline order. A missing index
/// keeps its slot; closing the gap drifts every later stamp by a minute.
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

/// Words captured so far. Counted from the segment text, never [`assemble`]'s
/// output — its `[hh:mm]` stamps would add a word per minute of meeting.
pub fn word_count(segments: &[Option<String>]) -> usize {
    segments
        .iter()
        .flatten()
        .map(|s| s.split_whitespace().count())
        .sum()
}

/// The note's markdown and title. **Any** failure still returns the raw
/// transcript verbatim: there is no error path out of here, because losing an
/// hour of meeting to a 429 is the worst outcome this code could produce.
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

/// Keeps the running transcript on disk while a meeting records, so a crash or
/// a quit loses at most the chunk being transcribed. Written beside and renamed
/// over, so a crash mid-write leaves the previous draft, never half of one.
pub fn save_draft(path: &Path, transcript: &str) -> Result<()> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, transcript)?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

/// Saves a draft a meeting left behind as a note, then removes it. None when
/// there is nothing to recover.
///
/// ponytail: the raw transcript, unsummarised — this runs at launch, before
/// anyone is waiting on a network round-trip. Summarising on open is the
/// upgrade if recovered notes turn out to be common.
pub fn recover_draft(draft: &Path, notes_dir: &Path) -> Result<Option<PathBuf>> {
    let transcript = match std::fs::read_to_string(draft) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if transcript.trim().is_empty() {
        std::fs::remove_file(draft)?;
        return Ok(None);
    }
    // Stamped when the meeting last wrote, not now: it files beside the day
    // it happened.
    let at = std::fs::metadata(draft)
        .and_then(|m| m.modified())
        .map(chrono::DateTime::<chrono::Utc>::from)
        .unwrap_or_else(|_| chrono::Utc::now());
    let note = format!(
        "# Recovered meeting\n\n_Aloud quit before this meeting was saved; this is the transcript up to then._\n\n## Transcript\n\n{transcript}"
    );
    let path = aloud_notes::write_note(notes_dir, "Recovered meeting", &note, at)?;
    // Only once the note exists: removing first would lose it to a full disk.
    std::fs::remove_file(draft)?;
    Ok(Some(path))
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

    /// A meeting the app quit on must come back as a note on the next launch,
    /// once — a draft left in place would be recovered again every launch.
    #[test]
    fn a_left_over_draft_becomes_a_note_once() {
        let dir = tempfile::tempdir().unwrap();
        let draft = dir.path().join("meeting-draft.md");
        let notes = dir.path().join("Notes");

        assert_eq!(recover_draft(&draft, &notes).unwrap(), None);

        save_draft(&draft, "[00:00] first minute").unwrap();
        save_draft(&draft, "[00:00] first minute\n\n[00:01] second").unwrap();
        let note = recover_draft(&draft, &notes).unwrap().expect("a note");
        let text = std::fs::read_to_string(note).unwrap();
        assert!(text.contains("[00:01] second"), "{text}");
        assert!(!draft.exists());
        assert_eq!(recover_draft(&draft, &notes).unwrap(), None);
    }

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

    /// Sorting by completion order would silently shuffle the timeline.
    #[test]
    fn out_of_order_chunks_reassemble_by_position() {
        let segments = in_order(vec![
            (2, "third minute".to_string()),
            (0, "first minute".to_string()),
        ]);

        // Index 1 never arrived, so it must stay an empty slot.
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1], None);
        assert_eq!(
            assemble(&segments),
            "[00:00] first minute\n\n[00:02] third minute"
        );
    }

    /// Guards: counting `assemble()`'s output added the `[hh:mm]` stamps, so a
    /// 90-minute meeting read 90 words longer than it was.
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

    /// A failed summary must never cost the transcript. The base URL refuses
    /// connections, so this is the real error path, not a stub.
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
