//! Voice macros: spoken shorthand that expands to canned text.
//!
//! Expansion runs on the *cleaned* transcript, so the model has already fixed
//! spacing and casing before a trigger is matched, and one dictation can
//! contain several. Matching is whole-phrase, case-insensitive and anchored on
//! word boundaries, so "my email" does not fire inside "customsummary email".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Macro {
    /// The spoken phrase that triggers expansion, e.g. "my address".
    pub trigger: String,
    pub expansion: String,
}

/// Expands every macro whose trigger appears in `text`, longest trigger first so
/// "my work email" wins over "my email".
pub fn expand(text: &str, macros: &[Macro]) -> String {
    let mut ordered: Vec<&Macro> = macros
        .iter()
        .filter(|m| !m.trigger.trim().is_empty())
        .collect();
    // Chars, not bytes: a smart quote is three bytes, which would rank a
    // possessive trigger above a longer one that has none.
    ordered.sort_by_key(|m| std::cmp::Reverse(m.trigger.trim().chars().count()));

    let mut result = text.to_string();
    for m in ordered {
        result = replace_phrase(&result, m.trigger.trim(), &m.expansion);
    }
    result
}

/// Case-insensitive, word-boundary replacement of `phrase` with `to`. Hand-rolled
/// rather than a regex dependency for one pattern; the boundary rule is just
/// "not flanked by an alphanumeric".
///
/// Compared over [`fold`]ed chars rather than bytes, then mapped back through
/// `origin` to slice the original text — the fold drops and merges characters,
/// so the two are not positionally the same.
fn replace_phrase(haystack: &str, phrase: &str, to: &str) -> String {
    let hay: Vec<char> = haystack.chars().collect();
    let (folded, origin) = fold(&hay);
    let (needle, _) = fold(&phrase.chars().collect::<Vec<_>>());
    if needle.is_empty() {
        return haystack.to_string();
    }

    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    let mut i = 0;
    while i + needle.len() <= folded.len() {
        let end = i + needle.len();
        // Where the match starts and ends in the original, which spans any
        // characters the fold dropped inside it.
        let from = origin[i];
        let to_ = origin.get(end).copied().unwrap_or(hay.len());
        let matched = folded[i..end] == needle[..]
            && (from == 0 || !hay[from - 1].is_alphanumeric())
            && (to_ == hay.len() || !hay[to_].is_alphanumeric());
        if matched {
            out.extend(&hay[cursor..from]);
            out.push_str(to);
            cursor = to_;
            i = end;
        } else {
            i += 1;
        }
    }
    out.extend(&hay[cursor..]);
    out
}

/// Folds away the differences between a trigger typed into Settings and the
/// same words coming back from Whisper, alongside each folded char's index in
/// `text` — the fold is not 1:1, so the caller cannot recover them.
///
/// Apostrophes are the reason this exists: WKWebView applies macOS smart-quote
/// substitution inside text inputs, so a trigger typed as "Sam's email" is
/// stored as U+2019 while a transcript spells it U+0027, or drops it entirely.
/// Removing it rather than normalising it makes all three the same trigger. The
/// cost is that a trigger of "well" also fires on "we'll"; single common words
/// make poor triggers anyway.
///
/// `to_lowercase().next()` keeps Unicode case folding for the 1:1 majority and
/// degrades to the first char for the few that expand (İ → i); both sides fold
/// identically, so the comparison stays consistent either way.
fn fold(text: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut folded = Vec::with_capacity(text.len());
    let mut origin = Vec::with_capacity(text.len());
    for (i, &c) in text.iter().enumerate() {
        let f = match c {
            '\'' | '\u{2018}' | '\u{2019}' | '\u{02BC}' => continue,
            // A run of spaces, or a line break cleanup put mid-phrase, is still
            // one word gap.
            c if c.is_whitespace() => {
                if folded.last() == Some(&' ') {
                    continue;
                }
                ' '
            }
            c => c.to_lowercase().next().unwrap_or(c),
        };
        folded.push(f);
        origin.push(i);
    }
    (folded, origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(trigger: &str, expansion: &str) -> Macro {
        Macro {
            trigger: trigger.into(),
            expansion: expansion.into(),
        }
    }

    #[test]
    fn expands_a_simple_trigger() {
        let macros = [m("my email", "jane@example.com")];
        assert_eq!(
            expand("Send it to my email please", &macros),
            "Send it to jane@example.com please"
        );
    }

    #[test]
    fn matching_is_case_insensitive() {
        let macros = [m("my address", "1 Infinite Loop")];
        assert_eq!(expand("My Address", &macros), "1 Infinite Loop");
    }

    #[test]
    fn does_not_fire_inside_a_larger_word() {
        let macros = [m("sig", "— Jane")];
        assert_eq!(
            expand("my signature is sig", &macros),
            "my signature is — Jane"
        );
    }

    #[test]
    fn longest_trigger_wins() {
        let macros = [
            m("my email", "personal@x.com"),
            m("my work email", "jane@work.com"),
        ];
        assert_eq!(expand("use my work email", &macros), "use jane@work.com");
    }

    #[test]
    fn blank_trigger_is_ignored() {
        let macros = [m("   ", "nope")];
        assert_eq!(expand("leave this alone", &macros), "leave this alone");
    }

    #[test]
    fn multiple_occurrences_all_expand() {
        let macros = [m("foo", "bar")];
        assert_eq!(expand("foo and foo", &macros), "bar and bar");
    }

    #[test]
    fn no_macros_is_the_identity() {
        assert_eq!(expand("unchanged", &[]), "unchanged");
    }

    /// Regression: the Settings input smart-quotes a typed apostrophe, so this
    /// is the shape every possessive trigger actually has on disk.
    #[test]
    fn a_smart_quoted_trigger_matches_a_plain_transcript() {
        let macros = [m("Sam\u{2019}s email", "sam@example.com")];
        assert_eq!(
            expand("Send it to Sam's email please", &macros),
            "Send it to sam@example.com please"
        );
    }

    /// And the reverse: Whisper punctuates with a curly apostrophe often enough
    /// that folding only the trigger would leave the same hole.
    #[test]
    fn a_plain_trigger_matches_a_smart_quoted_transcript() {
        let macros = [m("sam's email", "sam@example.com")];
        assert_eq!(
            expand("send it to Sam\u{2019}s email", &macros),
            "send it to sam@example.com"
        );
    }

    /// Whisper is not reliable about the possessive apostrophe on a name.
    #[test]
    fn a_dropped_apostrophe_still_matches() {
        let macros = [m("Sam\u{2019}s email", "sam@example.com")];
        assert_eq!(
            expand("send it to Sams email", &macros),
            "send it to sam@example.com"
        );
    }

    /// The fold drops characters inside the match, so the replaced span is
    /// wider than the trigger — a mapping bug shows up as a stray apostrophe.
    #[test]
    fn nothing_of_the_match_is_left_behind() {
        let macros = [m("sams email", "sam@example.com")];
        assert_eq!(expand("[Sam\u{2019}s email]", &macros), "[sam@example.com]");
    }

    /// Cleanup can break a line anywhere, including between a trigger's words.
    #[test]
    fn a_line_break_inside_the_trigger_still_matches() {
        let macros = [m("my email", "jane@example.com")];
        assert_eq!(
            expand("send my\nemail now", &macros),
            "send jane@example.com now"
        );
    }

    /// Byte-indexed matching read the boundary off the first byte of a
    /// multi-byte char, which is never ASCII-alphanumeric — so any trigger
    /// ending a word before an accent fired.
    #[test]
    fn a_multibyte_char_is_still_a_word_boundary() {
        let macros = [m("caf", "COFFEE")];
        assert_eq!(expand("caf\u{e9} au lait", &macros), "café au lait");
    }

    #[test]
    fn text_around_the_match_keeps_its_original_characters() {
        let macros = [m("sig", "— Jane")];
        assert_eq!(
            expand("It\u{2019}s CAF\u{c9}, sig", &macros),
            "It\u{2019}s CAF\u{c9}, — Jane"
        );
    }

    #[test]
    fn expansion_can_be_multiline() {
        let macros = [m("standup", "Yesterday:\nToday:\nBlockers:")];
        assert_eq!(expand("standup", &macros), "Yesterday:\nToday:\nBlockers:");
    }
}
