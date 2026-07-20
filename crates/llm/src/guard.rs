//! Catching the cleanup model *answering* the dictation instead of cleaning it.
//!
//! Dictate "write an email to Sam about Friday" and a model that reads the
//! transcript as an instruction returns the email. It looks like a great
//! response and it is the wrong text entirely — and it lands in someone's
//! document before the user can read it.
//!
//! **Ported verbatim from `appearsToHaveExecutedInstruction` in
//! `PostProcessingService.swift`,** per the port-intent convention: the marker
//! list, the stop words, the 0.35 overlap ratio and the assistant-preamble
//! pattern were all tuned against real failures on real models. Every one of
//! them trades a missed catch against a false positive that throws away a
//! *good* cleanup, and the balance is not re-derivable from first principles.
//! This heuristic was ported, not designed. Do not retune it without the same
//! evidence.
//!
//! **What it does not catch, measured against live Groq.** Given
//! "write an email to Sam letting him know the deploy slipped to Friday",
//! gpt-oss-20b returned a formatted email that reuses the dictated nouns —
//! Sam, Friday, slipped, release notes. That is a 0.62 overlap and no
//! preamble, so the guard passes it. It catches the model *chatting back*
//! ("Sure! Here's a draft…") and the model answering with facts the transcript
//! never contained; it does not catch an instruction executed in the
//! transcript's own vocabulary. Widening it means lowering [`OVERLAP_RATIO`],
//! and the same run showed why not: a real transcript *about* writing an email
//! sits in the same range. Left as ported.

use std::collections::HashSet;

/// Words that make a transcript look like it is addressing an assistant. A
/// transcript containing none of these is never judged, however little it
/// overlaps its cleanup — that is what keeps an aggressive tidy-up from
/// tripping the guard.
const INSTRUCTION_MARKERS: &[&str] = &[
    "ask",
    "answer",
    "compose",
    "create",
    "draft",
    "email",
    "generate",
    "make",
    "message",
    "prompt",
    "reply",
    "respond",
    "response",
    "summarize",
    "tell",
    "translate",
    "write",
    "claude",
    "chatgpt",
    "ai",
    "llm",
];

/// Dropped before comparing, so "the" surviving cleanup is not read as the
/// transcript surviving cleanup.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "could", "for", "from", "had",
    "has", "have", "he", "her", "him", "his", "i", "if", "in", "into", "is", "it", "its", "just",
    "me", "my", "of", "on", "or", "our", "please", "she", "so", "that", "the", "their", "them",
    "then", "there", "this", "to", "um", "uh", "was", "we", "were", "what", "when", "where", "who",
    "with", "would", "you", "your",
];

/// The Swift regex, verbatim:
/// `(?i)^\s*(sure|certainly|absolutely|here(?:'s| is)|i(?:'d| would) be happy to|i can)\b`
///
/// Written out as prefixes rather than compiled: the pattern is a plain
/// case-insensitive alternation anchored at the start, so `regex` would be a
/// new dependency for a `starts_with` loop. The `\b` is the boundary check in
/// [`has_assistant_preamble`].
const ASSISTANT_PREAMBLES: &[&str] = &[
    "sure",
    "certainly",
    "absolutely",
    "here's",
    "here is",
    "i'd be happy to",
    "i would be happy to",
    "i can",
];

/// Below this share of significant words surviving, the cleanup is no longer a
/// tidied version of what was said.
const OVERLAP_RATIO: f64 = 0.35;

/// Whether `cleaned` looks like an answer to `raw` rather than a tidied copy
/// of it.
///
/// Returns false while translating: a translation legitimately shares almost
/// no tokens with its source, which is exactly the signal the overlap test
/// reads as an executed instruction. The guard would fire on every single
/// translated dictation.
pub fn appears_to_have_executed_instruction(
    raw: &str,
    cleaned: &str,
    output_language: &str,
) -> bool {
    if !output_language.trim().is_empty() {
        return false;
    }

    let raw_tokens = significant_tokens(raw);
    let cleaned_tokens = significant_tokens(cleaned);
    if raw_tokens.is_empty() || cleaned_tokens.is_empty() {
        return false;
    }

    let raw_markers: Vec<&str> = raw_tokens
        .iter()
        .map(String::as_str)
        .filter(|t| INSTRUCTION_MARKERS.contains(t))
        .collect();
    if raw_markers.is_empty() {
        return false;
    }

    let preserved_markers = raw_markers.iter().any(|m| cleaned_tokens.contains(*m));
    let overlap = raw_tokens
        .iter()
        .filter(|t| cleaned_tokens.contains(*t))
        .count();
    let overlap_ratio = overlap as f64 / raw_tokens.len().max(1) as f64;

    // The preamble has to be absent from the raw text to count. "Sure, send it
    // Friday" is a dictation that legitimately starts that way, and its
    // cleanup keeps the word.
    let answered_aloud = has_assistant_preamble(cleaned) && !has_assistant_preamble(raw);

    answered_aloud || (!preserved_markers && overlap_ratio < OVERLAP_RATIO)
}

/// Lowercased words of more than one character, minus the stop words.
fn significant_tokens(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1 && !STOP_WORDS.contains(t))
        .map(str::to_string)
        .collect()
}

fn has_assistant_preamble(text: &str) -> bool {
    let start = text.trim_start().to_lowercase();
    ASSISTANT_PREAMBLES.iter().any(|preamble| {
        start
            .strip_prefix(preamble)
            // `\b`: the alternative has to end on a word boundary, or "I canvas
            // the team on Friday" matches "i can".
            .is_some_and(|rest| rest.chars().next().is_none_or(|c| !is_word_char(c)))
    })
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this exists for: the transcript is an instruction, and the
    /// model carried it out. What would have been pasted is an email nobody
    /// asked to have written for them.
    #[test]
    fn catches_the_model_writing_the_email_it_was_asked_about() {
        let raw = "write an email to Sam letting him know the deploy slipped to \
                   Friday and we will send the release notes after";
        let cleaned = "Sure! Here's a draft:\n\nHi Sam,\n\nQuick heads-up that the \
                       deployment has slipped to Friday. We'll follow up with the \
                       release notes once it's out.\n\nBest,";
        assert!(appears_to_have_executed_instruction(raw, cleaned, ""));
    }

    /// No preamble at all — the model just answered. Caught by the overlap
    /// test: none of the instruction markers survive and almost no words do.
    #[test]
    fn catches_a_bare_answer_with_no_preamble() {
        let raw = "summarize the quarterly numbers for the board deck and tell me \
                   which region grew fastest";
        let cleaned = "APAC grew fastest at 34% year over year, ahead of EMEA at \
                       12%. Total bookings finished at $8.2M.";
        assert!(appears_to_have_executed_instruction(raw, cleaned, ""));
    }

    /// The one that must not fire. Dictation *about* writing an email is the
    /// most common transcript containing "write" and "email", and its cleanup
    /// keeps every word that matters.
    #[test]
    fn leaves_a_normal_dictation_about_writing_an_email_alone() {
        let raw = "um so I was going to write an email to Sam about the deploy but \
                   I think you know a message is faster";
        let cleaned = "I was going to write an email to Sam about the deploy, but I \
                       think a message is faster.";
        assert!(!appears_to_have_executed_instruction(raw, cleaned, ""));
    }

    /// A transcript with no instruction marker is never judged, however hard
    /// the cleanup rewrote it — otherwise heavy filler removal trips the guard.
    #[test]
    fn a_transcript_with_no_marker_is_never_judged() {
        let raw = "um yeah so like the uh thing with the uh Friday thing you know";
        let cleaned = "Yes, regarding Friday.";
        assert!(!appears_to_have_executed_instruction(raw, cleaned, ""));
    }

    /// Translating rewrites every word by design. Left on, the guard fired on
    /// every translated dictation and pasted the untranslated transcript.
    #[test]
    fn the_guard_self_disables_while_translating() {
        let raw = "write an email to Sam letting him know the deploy slipped to Friday";
        let cleaned = "Schreibe Sam eine E-Mail, dass das Deployment auf Freitag \
                       verschoben wurde.";
        assert!(appears_to_have_executed_instruction(raw, cleaned, ""));
        assert!(!appears_to_have_executed_instruction(
            raw, cleaned, "German"
        ));
        assert!(
            appears_to_have_executed_instruction(raw, cleaned, "  \n"),
            "a blank-but-whitespace language is not translating"
        );
    }

    /// A dictation that genuinely opens with "Sure," keeps it through cleanup.
    /// Reading that as a preamble throws away a correct result.
    #[test]
    fn a_preamble_already_in_the_raw_transcript_is_not_the_models() {
        let raw = "sure tell Sam we can ship it Friday if the tests pass";
        let cleaned = "Sure, tell Sam we can ship it Friday if the tests pass.";
        assert!(!appears_to_have_executed_instruction(raw, cleaned, ""));
    }

    /// `\b`, the reason the pattern is not a bare `starts_with`.
    #[test]
    fn preamble_matching_stops_at_a_word_boundary() {
        assert!(has_assistant_preamble("Sure, here you go"));
        assert!(has_assistant_preamble("  Here's the draft"));
        assert!(has_assistant_preamble("Here is the draft"));
        assert!(has_assistant_preamble("I'd be happy to help"));
        assert!(has_assistant_preamble("I can do that"));
        assert!(!has_assistant_preamble("Surely we ship Friday"));
        assert!(!has_assistant_preamble("I canvas the team on Friday"));
        assert!(!has_assistant_preamble("Absolutely_certain"));
        assert!(!has_assistant_preamble("The answer is sure"));
    }

    #[test]
    fn empty_text_on_either_side_is_not_a_verdict() {
        assert!(!appears_to_have_executed_instruction(
            "",
            "Sure! Here you go.",
            ""
        ));
        assert!(!appears_to_have_executed_instruction(
            "write an email to Sam",
            "",
            ""
        ));
    }
}
