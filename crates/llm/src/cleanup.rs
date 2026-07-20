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
    /// Language the *pasted* text should be in, blank to leave it alone.
    ///
    /// A post-processing instruction, never a transcription parameter — and
    /// deliberately not passed to `aloud_transcribe`. Whisper's `translate`
    /// task only ever targets English, so it cannot express "into German", and
    /// translating before cleanup would hand the cleanup prompt text in a
    /// language its rules were not written for. voice-dict reached the same
    /// conclusion: `outputLanguage` threads through `PostProcessingService`
    /// and nowhere near the transcriber.
    pub output_language: &'a str,
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
        Message::system(system_prompt(
            options.custom_prompt,
            options.output_language,
        )),
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
///
/// Public because the Test Prompt runner has to say so: a short sample returns
/// verbatim, which looks exactly like a prompt that did nothing.
pub fn is_too_short(raw: &str) -> bool {
    raw.split_whitespace().count() <= MIN_WORDS
}

/// The custom prompt *replaces* the built-in one rather than adding to it —
/// an app rule's style hint is appended upstream, before it reaches here.
/// A target language is appended to whichever prompt won, verbatim from
/// voice-dict's `applyOutputLanguage`.
fn system_prompt(custom: &str, output_language: &str) -> String {
    let mut prompt = if custom.trim().is_empty() {
        prompts::CLEANUP.to_string()
    } else {
        custom.to_string()
    };
    let language = output_language.trim();
    if !language.is_empty() {
        prompt.push_str(&format!(
            "\n\nIMPORTANT: Translate the final cleaned text into {language}. Output ONLY in {language}, regardless of the original spoken language."
        ));
    }
    prompt
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
///
/// **The sentinel applies while translating too.** An earlier port disabled it
/// whenever an output language was set, following Swift's
/// `sanitizeVerbatimTranslation` — but that function belongs to a *different*
/// Swift path. `verbatimTranslationSystemPrompt` is a minimal literal
/// translator carrying no cleanup rules and no sentinel instruction, which is
/// the precondition its comment names. The path this crate implements is
/// Swift's `applyOutputLanguage`, whose reply goes to
/// `sanitizePostProcessedTranscript` — and that one *does* strip the sentinel.
///
/// It has to: `system_prompt` appends the translation instruction to the
/// cleanup prompt rather than replacing it, so the sentinel instruction is
/// still in play and filler-only speech still returns it. Skipping the check
/// meant every "um, uh" while translating typed EMPTY into the document.
///
/// The exact match is what makes it safe in both directions: the sentinel is
/// the whole output, all caps, untranslated. A real translation is a sentence,
/// and even a single-word one is lowercase, so it does not collide.
fn interpret(cleaned: &str) -> String {
    if cleaned.trim() == EMPTY_SENTINEL {
        return String::new();
    }
    cleaned.to_string()
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

    /// The failure this guards: the first port disabled the sentinel whenever
    /// an output language was set, to avoid swallowing a translation that
    /// happened to be the word "empty". But filler-only speech still makes the
    /// model emit the sentinel while translating, so every "um, uh" typed
    /// EMPTY into the user's document.
    #[test]
    fn the_sentinel_still_means_paste_nothing_while_translating() {
        assert_eq!(interpret(EMPTY_SENTINEL), "");
        // A real translation is a sentence, or at worst a lowercase word, so
        // the exact all-caps match does not collide with one.
        assert_eq!(interpret("Der Tank ist leer."), "Der Tank ist leer.");
        assert_eq!(interpret("empty"), "empty");
    }

    #[test]
    fn a_blank_custom_prompt_falls_back_to_the_built_in_one() {
        assert_eq!(system_prompt("", ""), prompts::CLEANUP);
        assert_eq!(system_prompt("   \n", ""), prompts::CLEANUP);
        assert_eq!(system_prompt("be terse", ""), "be terse");
    }

    /// The target language is appended to whichever prompt won, not swapped
    /// for it — a custom prompt that also translates has to keep both.
    #[test]
    fn a_target_language_is_appended_to_either_prompt() {
        let custom = system_prompt("be terse", "French");
        assert!(custom.starts_with("be terse"));
        assert!(custom.contains("Output ONLY in French"));

        let built_in = system_prompt("", "Japanese");
        assert!(built_in.starts_with(prompts::CLEANUP));
        assert!(built_in.contains("Translate the final cleaned text into Japanese"));

        assert_eq!(system_prompt("be terse", "  \n"), "be terse");
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
            output_language: "",
        };
        assert_eq!(clean(options, "one sec").await.unwrap(), "one sec");
    }
}
