//! Ask mode: answering a spoken question instead of pasting it.
//!
//! Ported from `AskService.swift`.

/// Spoken prefixes that route a dictation into Ask mode.
///
/// Each entry lists the accepted spellings for one spoken word, because ASR
/// mishears short names constantly: "hey flow" really is transcribed
/// "Hey, Flo" and "hey, flo'" by whisper. Matching only the exact spelling
/// makes the trigger unusable in practice.
///
/// ponytail: a constant list, as in the Swift app. Make it a setting when
/// someone actually wants a different phrase.
pub const TRIGGER_PHRASES: &[&[&[&str]]] = &[
    &[&["hey"], &["flow", "flo", "floe"]],
    &[&["ask"], &["ai", "ay", "a"]],
];

/// System prompt for a spoken question. Copied verbatim from the Swift app.
pub const SYSTEM_PROMPT: &str =
    "You are a concise assistant answering a spoken question. APP_CONTEXT describes \
what is on the user's screen right now; use it when the question refers to it \
(e.g. \"what does this error mean?\") and ignore it otherwise. Reply in simple \
markdown — **bold**, `inline code`, short bullet lists, fenced code blocks — \
only where it genuinely helps; no tables. Keep the answer under 150 \
words unless the question truly demands more. The question is a transcription \
of speech and may contain minor errors; answer the intent.";

/// The question, if `transcript` opens with a spoken trigger phrase.
///
/// Only matches at the start: "Send hey flow to the team" is dictation, not a
/// question. Each word is compared with punctuation stripped, because the
/// transcriber attaches it freely ("Hey, flow!").
pub fn question_from_trigger(transcript: &str) -> Option<String> {
    crate::trigger::after_trigger(transcript, TRIGGER_PHRASES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_trigger_and_its_punctuation() {
        assert_eq!(
            question_from_trigger("Hey Flow, what does this error mean?").as_deref(),
            Some("what does this error mean?")
        );
        assert_eq!(
            question_from_trigger("  hey flow. What's the capital of France").as_deref(),
            Some("What's the capital of France")
        );
        assert_eq!(
            question_from_trigger("Ask AI how do I exit vim").as_deref(),
            Some("how do I exit vim")
        );
    }

    /// The exact transcript whisper produced for a real spoken "hey flow":
    /// it hears "Flo". Demanding the dictionary spelling made the trigger
    /// fail on real speech, which is the only speech that matters.
    #[test]
    fn tolerates_asr_misspellings_of_the_trigger() {
        assert_eq!(
            question_from_trigger("Hey, Flo, what is the capital of France?").as_deref(),
            Some("what is the capital of France?")
        );
    }

    #[test]
    fn trigger_with_no_question_does_not_match() {
        assert_eq!(question_from_trigger("Hey flow"), None);
    }

    /// The important negatives: a trigger phrase inside a sentence is
    /// dictation, and pasting it is the correct behaviour.
    #[test]
    fn mid_sentence_mentions_do_not_match() {
        assert_eq!(question_from_trigger("Send hey flow to the team"), None);
        assert_eq!(question_from_trigger("Let's ask AI about it later"), None);
    }
}
