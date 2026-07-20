//! Turning a raw transcript into the text that actually gets pasted.
//!
//! Ported from `PostProcessingService.swift`. Every decision here can be
//! quietly wrong — the wrong prompt, a dropped vocabulary, a sentinel treated
//! as literal text — and none of them fail loudly: they just paste something
//! slightly off. That is why they live in a crate with tests rather than in
//! the app.

use crate::{prompts, LlmClient, Message, EMPTY_SENTINEL};
use anyhow::Result;
use tracing::Instrument;

/// Transcripts this short skip cleanup entirely.
///
/// The cleanup prompt is ~1,500 tokens and is spent on every dictation,
/// against a free-tier budget of 8,000 per minute — about five dictations
/// before a 429. On "yes", "one sec", "send it" there is nothing to clean, so
/// skipping those is most of the spend for none of the benefit. Whisper
/// punctuates a short utterance correctly on its own.
pub const MIN_WORDS: usize = 3;

/// What cleanup runs against. Borrowed, so this crate needs no settings store.
pub struct Options<'a> {
    pub api_key: &'a str,
    pub base_url: &'a str,
    pub model: &'a str,
    /// Tried when the primary is rate-limited. Blank is treated as absent.
    pub fallback: Option<&'a str>,
    /// Replaces the built-in prompt entirely when non-blank.
    pub custom_prompt: &'a str,
    /// Proper nouns and jargon the transcriber tends to mangle.
    pub vocabulary: &'a [String],
}

/// Cleans `raw`, or returns it unchanged when there is nothing worth spending
/// a request on.
///
/// Errors are the caller's to absorb: pasting the unpolished words beats
/// pasting nothing.
pub async fn clean(options: Options<'_>, raw: &str) -> Result<String> {
    if is_too_short(raw) {
        return Ok(raw.to_string());
    }

    let client = LlmClient::new(options.api_key, Some(options.base_url))?;
    let messages = [
        Message::system(system_prompt(options.custom_prompt)),
        Message::user(user_message(raw, options.vocabulary)),
    ];

    let cleaned = client
        .complete_with_fallback(
            options.model,
            options.fallback.filter(|f| !f.trim().is_empty()),
            &messages,
            Some(0.0), // deterministic: this is cleanup, not writing
        )
        .instrument(tracing::info_span!(
            "cleanup",
            model = %options.model,
            words = raw.split_whitespace().count(),
        ))
        .await?;

    Ok(interpret(&cleaned))
}

/// Whether the transcript is too short to be worth a cleanup request.
fn is_too_short(raw: &str) -> bool {
    raw.split_whitespace().count() <= MIN_WORDS
}

/// The custom prompt *replaces* the built-in one rather than adding to it —
/// an app rule's style hint is appended upstream, before it reaches here.
fn system_prompt(custom: &str) -> String {
    if custom.trim().is_empty() {
        prompts::CLEANUP.to_string()
    } else {
        custom.to_string()
    }
}

/// The transcript, prefixed with the vocabulary when there is one.
fn user_message(raw: &str, vocabulary: &[String]) -> String {
    if vocabulary.is_empty() {
        raw.to_string()
    } else {
        format!("VOCABULARY: {}\n\n{raw}", vocabulary.join(", "))
    }
}

/// The prompt specifies [`EMPTY_SENTINEL`] for filler-only input, which means
/// paste nothing. Returning it literally would type "EMPTY" into the user's
/// document.
fn interpret(cleaned: &str) -> String {
    if cleaned.trim() == EMPTY_SENTINEL {
        String::new()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_utterances_skip_the_request() {
        assert!(is_too_short("yes"));
        assert!(is_too_short("one sec"));
        assert!(is_too_short("send it now"), "three words is still short");
        assert!(!is_too_short("send it now please"));
        assert!(is_too_short(""));
        // Whitespace is not a word; this would otherwise buy a request for
        // nothing on a transcript of pure spaces.
        assert!(is_too_short("   \n  "));
    }

    /// The sentinel reaching the paste path types "EMPTY" into the document.
    #[test]
    fn the_empty_sentinel_means_paste_nothing() {
        assert_eq!(interpret(EMPTY_SENTINEL), "");
        assert_eq!(interpret("  EMPTY \n"), "", "trimmed, as the prompt emits");
        assert_eq!(
            interpret("EMPTY the calendar"),
            "EMPTY the calendar",
            "a transcript that merely starts with the word is not the sentinel"
        );
        assert_eq!(interpret("hello there"), "hello there");
    }

    #[test]
    fn a_blank_custom_prompt_falls_back_to_the_built_in_one() {
        assert_eq!(system_prompt(""), prompts::CLEANUP);
        assert_eq!(system_prompt("   \n"), prompts::CLEANUP);
        assert_eq!(system_prompt("be terse"), "be terse");
    }

    /// A dropped vocabulary is invisible: the transcript still reads fine,
    /// just with the user's proper nouns spelled wrong.
    #[test]
    fn vocabulary_is_prefixed_only_when_there_is_one() {
        assert_eq!(user_message("hello", &[]), "hello");
        assert_eq!(
            user_message("hello", &["Aloud".to_string(), "Tauri".to_string()]),
            "VOCABULARY: Aloud, Tauri\n\nhello"
        );
    }

    #[tokio::test]
    async fn a_short_transcript_returns_verbatim_without_a_request() {
        // The base URL refuses connections, so reaching the network at all
        // would fail this rather than silently pass.
        let options = Options {
            api_key: "k",
            base_url: "http://127.0.0.1:1",
            model: "m",
            fallback: None,
            custom_prompt: "",
            vocabulary: &[],
        };
        assert_eq!(clean(options, "one sec").await.unwrap(), "one sec");
    }
}
