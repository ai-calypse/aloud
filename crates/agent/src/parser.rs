//! The intent parser — Phase 1, Task 4.
//!
//! Turns clean spoken text into a structured [`ParsedAction`], using the action
//! catalog as the set of available tools. Returns None when nothing matches or
//! confidence is too low, so the caller can say "I didn't understand that"
//! rather than guess.

use crate::schema::{ParsedAction, ACTIONS};
use aloud_llm::{LlmClient, Message};
use anyhow::{Context, Result};

/// Below this the parse is treated as no-match: acting on a guess is worse than
/// admitting the command was not understood.
const MIN_CONFIDENCE: f32 = 0.6;

/// Parses `text` into an action, or None if it is not a recognized command.
pub async fn parse_intent(
    client: &LlmClient,
    model: &str,
    text: &str,
) -> Result<Option<ParsedAction>> {
    let messages = [
        Message::system(system_prompt()),
        Message::user(text.to_string()),
    ];

    // Deterministic: parsing a command is not a creative task.
    let raw = client
        .complete_with_fallback(model, None, &messages, Some(0.0))
        .await
        .context("intent parse request failed")?;

    let action = parse_response(&raw)?;
    Ok(action.filter(|a| a.is_known() && a.confidence >= MIN_CONFIDENCE))
}

/// Extracts the action from the model's reply.
///
/// The model is told to return a single JSON object or the literal `NONE`, so
/// this is lenient about surrounding prose but strict about the shape.
fn parse_response(raw: &str) -> Result<Option<ParsedAction>> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("none") || trimmed.is_empty() {
        return Ok(None);
    }

    // Pull the first {...} span, tolerating a code fence or a stray sentence.
    let json = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => return Ok(None),
    };

    match serde_json::from_str::<ParsedAction>(json) {
        Ok(action) => Ok(Some(action)),
        Err(e) => {
            tracing::warn!(%e, %json, "intent parse returned malformed JSON");
            Ok(None)
        }
    }
}

/// The system prompt, listing every catalog action as an available tool.
fn system_prompt() -> String {
    let mut catalog = String::from("Available actions:\n");
    for def in ACTIONS {
        catalog.push_str(&format!(
            "- {} ({}): {}. required: [{}]{}\n",
            def.action,
            def.app,
            def.description,
            def.required_entities.join(", "),
            if def.optional_entities.is_empty() {
                String::new()
            } else {
                format!(", optional: [{}]", def.optional_entities.join(", "))
            },
        ));
    }

    format!(
        "You convert a spoken command into a single JSON action.\n\n{catalog}\n\
Return exactly one JSON object with these fields:\n\
  action: one of the action names above\n\
  app: the app for that action\n\
  entities: an object with the required fields filled from the command\n\
  confidence: 0.0-1.0, how sure you are\n\
  requires_confirmation: true for any action that sends, creates, or changes something\n\n\
If the command does not match any action, reply with exactly: NONE\n\
Return only the JSON or NONE, nothing else. The command is a speech \
transcription and may contain minor errors; infer the intent."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clean_action_object() {
        let raw = r#"{"action":"spotify.play","app":"spotify","entities":{"query":"jazz"},"confidence":0.9,"requires_confirmation":false}"#;
        let action = parse_response(raw).unwrap().unwrap();
        assert_eq!(action.action, "spotify.play");
        assert_eq!(action.entities.get("query").unwrap(), "jazz");
    }

    #[test]
    fn tolerates_a_code_fence_and_prose() {
        let raw = "Here you go:\n```json\n{\"action\":\"spotify.pause\",\"app\":\"spotify\",\"entities\":{},\"confidence\":0.8,\"requires_confirmation\":false}\n```";
        let action = parse_response(raw).unwrap().unwrap();
        assert_eq!(action.action, "spotify.pause");
    }

    #[test]
    fn none_and_junk_return_no_action() {
        assert!(parse_response("NONE").unwrap().is_none());
        assert!(parse_response("none").unwrap().is_none());
        assert!(parse_response("").unwrap().is_none());
        assert!(parse_response("I'm not sure what you mean.")
            .unwrap()
            .is_none());
    }

    #[test]
    fn malformed_json_is_treated_as_no_match_not_an_error() {
        // A hard error here would surface as a crash; a miss is the right call.
        let result = parse_response(r#"{"action": "spotify.play", broken"#).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn system_prompt_lists_every_catalog_action() {
        let prompt = system_prompt();
        for def in ACTIONS {
            assert!(prompt.contains(def.action), "missing {}", def.action);
        }
    }
}
