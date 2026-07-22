//! Filters Whisper's canned hallucinations on silence: whisper-large-v3
//! confidently emits stock YouTube-outro phrases on near-silence, so they are
//! dropped when it also reports a high `no_speech_prob`.
//!
//! **Tuned on ~500 samples** across quiet and noisy environments, deliberately
//! conservative — a real spoken "thank you" has a very low no_speech_prob. Do
//! not retune without the same evidence.

/// Phrases Whisper invents on silence. Add a phrase here to filter more.
const HALLUCINATION_PHRASES: &[&str] = &[
    "thank you",
    "thank you for watching",
    "thank you very much",
    "thank you so much",
    "thanks for watching",
    "please subscribe",
    "like and subscribe",
    "subtitles by",
    "subtitles by the amara.org community",
    "you",
];

/// Above this, whisper is telling us the audio was probably silence.
const NO_SPEECH_THRESHOLD: f64 = 0.1;

/// Whether `text` is a hallucination, given whisper's no-speech probability for
/// the first segment. None (providers that omit segment metadata) keeps the
/// text: dropping real speech beats letting a stray "thank you" through.
pub fn is_hallucination(text: &str, no_speech_prob: Option<f64>) -> bool {
    let normalized = normalize(text);
    if !HALLUCINATION_PHRASES.contains(&normalized.as_str()) {
        return false;
    }

    match no_speech_prob {
        Some(prob) => prob >= NO_SPEECH_THRESHOLD,
        None => {
            tracing::info!(
                phrase = %normalized,
                "skipping hallucination filter: provider omitted no_speech_prob"
            );
            false
        }
    }
}

/// Lowercased and stripped of surrounding punctuation, so "Thank you." matches.
fn normalize(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_stock_phrase_on_silence() {
        assert!(is_hallucination("Thank you.", Some(0.9)));
        assert!(is_hallucination("thanks for watching", Some(0.5)));
    }

    /// A really spoken "thank you" has a very low no_speech_prob and must survive.
    #[test]
    fn keeps_stock_phrase_when_really_spoken() {
        assert!(!is_hallucination("Thank you.", Some(0.01)));
    }

    #[test]
    fn keeps_normal_speech_regardless_of_probability() {
        assert!(!is_hallucination("send the report by friday", Some(0.99)));
    }

    #[test]
    fn keeps_text_when_probability_is_unknown() {
        assert!(!is_hallucination("Thank you.", None));
    }

    #[test]
    fn normalizes_case_and_punctuation() {
        assert_eq!(normalize("  Thank You!  "), "thank you");
        assert_eq!(normalize("You?"), "you");
    }

    #[test]
    fn threshold_is_inclusive() {
        assert!(is_hallucination("you", Some(NO_SPEECH_THRESHOLD)));
        assert!(!is_hallucination("you", Some(NO_SPEECH_THRESHOLD - 0.001)));
    }
}
