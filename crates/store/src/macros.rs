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
    ordered.sort_by_key(|m| std::cmp::Reverse(m.trigger.trim().len()));

    let mut result = text.to_string();
    for m in ordered {
        result = replace_phrase(&result, m.trigger.trim(), &m.expansion);
    }
    result
}

/// Case-insensitive, word-boundary replacement of `phrase` with `to`. Hand-rolled
/// rather than a regex dependency for one pattern; the boundary rule is just
/// "not flanked by an alphanumeric".
fn replace_phrase(haystack: &str, phrase: &str, to: &str) -> String {
    let hay_lower = haystack.to_lowercase();
    let needle = phrase.to_lowercase();
    let bytes = haystack.as_bytes();

    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    while let Some(rel) = hay_lower[cursor..].find(&needle) {
        let start = cursor + rel;
        let end = start + needle.len();

        let before_ok = start == 0 || !is_alnum(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_alnum(bytes[end]);

        if before_ok && after_ok {
            out.push_str(&haystack[cursor..start]);
            out.push_str(to);
            cursor = end;
        } else {
            // Advance past this false match to avoid a loop.
            out.push_str(&haystack[cursor..=start]);
            cursor = start + 1;
        }
    }
    out.push_str(&haystack[cursor..]);
    out
}

fn is_alnum(b: u8) -> bool {
    b.is_ascii_alphanumeric()
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

    #[test]
    fn expansion_can_be_multiline() {
        let macros = [m("standup", "Yesterday:\nToday:\nBlockers:")];
        assert_eq!(expand("standup", &macros), "Yesterday:\nToday:\nBlockers:");
    }
}
