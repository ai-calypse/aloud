//! System prompts and response cleanup.
//!
//! The prompts are the Swift app's verbatim, kept as separate files so they
//! diff cleanly and can be tuned without touching Rust. They encode a lot of
//! hard-won behaviour (self-corrections, instruction preservation, email
//! formatting) — treat changes as behavioural, not cosmetic.

/// Cleans a raw dictation transcript. Returns `EMPTY` for pure filler.
pub const CLEANUP: &str = include_str!("prompts/cleanup.md");

/// Transforms selected text per a spoken editing command.
pub const COMMAND: &str = include_str!("prompts/command.md");

/// Turns a meeting transcript into structured markdown notes.
pub const MEETING: &str = include_str!("prompts/meeting.md");

/// Removes `<think>...</think>` blocks that reasoning models emit before their
/// answer. Ported from `ModelConfiguration.stripThinkTags`.
pub fn strip_think_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            // Unterminated: the model was cut off mid-thought, so drop the rest
            // rather than pasting a partial monologue.
            None => return out.trim().to_string(),
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_present() {
        assert!(CLEANUP.contains("literal dictation cleanup layer"));
        assert!(CLEANUP.contains("return exactly: EMPTY"));
        assert!(COMMAND.contains("SELECTED_TEXT"));
    }

    #[test]
    fn strips_think_block() {
        assert_eq!(
            strip_think_tags("<think>hmm, maybe</think>Hello there"),
            "Hello there"
        );
    }

    #[test]
    fn strips_multiple_blocks() {
        assert_eq!(
            strip_think_tags("<think>a</think>Hi <think>b</think>there"),
            "Hi there"
        );
    }

    /// A cut-off model must not leak its monologue into the paste.
    #[test]
    fn drops_unterminated_block() {
        assert_eq!(strip_think_tags("Hello <think>wait, actually"), "Hello");
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(strip_think_tags("  Just text  "), "Just text");
    }
}
