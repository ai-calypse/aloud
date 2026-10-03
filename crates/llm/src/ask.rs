//! The spoken trigger that turns a dictation into a command: "hey flow, …".

/// Spoken prefixes that route a dictation into Ask mode.
///
/// Each entry lists the accepted spellings for one spoken word: whisper really
/// does transcribe "hey flow" as "Hey, Flo", and demanding the exact spelling
/// makes the trigger unusable.
///
/// ponytail: a constant list, as in the Swift app. Make it a setting when
/// someone actually wants a different phrase.
pub const TRIGGER_PHRASES: &[&[&[&str]]] = &[
    &[&["hey"], &["flow", "flo", "floe"]],
    &[&["ask"], &["ai", "ay", "a"]],
];

/// The question, if `transcript` opens with a spoken trigger phrase.
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

    /// The exact transcript whisper produced for a real spoken "hey flow".
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

    /// A trigger phrase inside a sentence is dictation; pasting it is correct.
    #[test]
    fn mid_sentence_mentions_do_not_match() {
        assert_eq!(question_from_trigger("Send hey flow to the team"), None);
        assert_eq!(question_from_trigger("Let's ask AI about it later"), None);
    }
}
