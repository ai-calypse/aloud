//! Matching a spoken trigger phrase at the start of a transcript.
//!
//! Ask ("hey flow") and Agent ("hey agent") both offer a spoken way in
//! alongside their hotkey, and both need the same three rules: match only at
//! the start, compare with punctuation stripped, and require something after
//! the phrase. Lives here rather than in either caller because it was written
//! twice — identically, down to the `normalize` helper — and a fix to one copy
//! would not have reached the other.

/// The remainder of `transcript` after any of `phrases` matches at the start.
///
/// A phrase is a list of word positions, each holding the spellings the
/// transcriber might produce for that word: `&[&["hey"], &["flow", "flo"]]`
/// matches "hey flow" and "hey flo".
///
/// Only matches at the start — "Send hey flow to the team" is dictation, not a
/// trigger. Returns None when nothing follows the phrase, so the phrase alone
/// is never treated as a request.
pub fn after_trigger(transcript: &str, phrases: &[&[&[&str]]]) -> Option<String> {
    let words: Vec<&str> = transcript.split_whitespace().collect();

    for phrase in phrases {
        if words.len() <= phrase.len() {
            continue; // the phrase alone is not a request
        }
        let matches = words
            .iter()
            .zip(*phrase)
            .all(|(word, spellings)| spellings.contains(&normalize(word).as_str()));
        if !matches {
            continue;
        }

        let rest = words[phrase.len()..]
            .join(" ")
            .trim_start_matches([',', '.', ' '])
            .to_string();
        return (!rest.is_empty()).then_some(rest);
    }
    None
}

/// Lowercased, letters and digits only — so "Hey," matches "hey". The
/// transcriber attaches punctuation freely.
fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHRASES: &[&[&[&str]]] = &[&[&["hey"], &["flow", "flo"]]];

    #[test]
    fn normalizes_punctuation_and_case() {
        assert_eq!(normalize("Hey,"), "hey");
        assert_eq!(normalize("FLOW!"), "flow");
    }

    #[test]
    fn matches_only_at_the_start() {
        assert_eq!(
            after_trigger("hey flow what is rust", PHRASES).as_deref(),
            Some("what is rust")
        );
        assert_eq!(after_trigger("send hey flow to the team", PHRASES), None);
    }

    #[test]
    fn the_phrase_alone_is_not_a_request() {
        assert_eq!(after_trigger("hey flow", PHRASES), None);
        assert_eq!(after_trigger("Hey, flow!", PHRASES), None);
    }

    #[test]
    fn strips_punctuation_between_the_phrase_and_the_rest() {
        assert_eq!(
            after_trigger("Hey, flow, what is rust", PHRASES).as_deref(),
            Some("what is rust")
        );
    }
}
