//! The Composio tool loop: model ↔ tools until the model produces a final
//! answer. A direct port of voice-dict's `AskService.answerUsingTools`.
//!
//! The model is told which Gmail/GitHub tools it has; it either answers or asks
//! to call tools; each call executes through Composio and the result is fed
//! back. Bounded rounds — the last round forbids tools so the model must answer
//! rather than loop forever. Write tools (send email, create issue) pause for an
//! explicit confirmation before they run; that is the one non-negotiable gate.

use crate::composio::{ComposioClient, ToolDef};
use aloud_llm::{LlmClient, LlmError, Message};
use anyhow::Result;
use std::future::Future;
use std::time::Duration;

const MAX_ROUNDS: usize = 5;

/// Progress events for the panel, in AG-UI tool-call vocabulary. The caller
/// maps these onto its own UI events.
#[derive(Debug, Clone)]
pub enum LoopEvent {
    /// The model asked to run a tool (with its parsed args as display lines).
    ToolCallStart { name: String, args: Vec<String> },
    /// That tool finished (success carries no payload; the model gets the JSON).
    ToolCallResult { name: String, success: bool },
    /// The chat API throttled us; the loop is waiting this long to retry once.
    RateLimited { seconds: u64 },
}

const TOOL_SYSTEM_PROMPT: &str = "\
You are a concise assistant acting on a spoken command. You can call tools to \
act on the user's Gmail and GitHub (send email, create drafts, fetch emails, \
create issues, list issues, get the authenticated user). Only call a tool when \
the command asks for such an action or needs that data; otherwise just answer. \
The command is a transcription of speech and may contain minor errors; act on \
the intent. After acting, confirm what you did in one short plain sentence. \
If a tool result was not successful or contains an error, say the action \
failed and why — never claim it worked.";

/// Runs the loop for `command`. Returns the model's final answer.
///
/// `on_event` fires for each tool call so the panel can show progress.
/// `confirm` is awaited before any write tool runs; its future resolves to the
/// user's yes/no.
pub async fn run<F>(
    llm: &LlmClient,
    model: &str,
    composio: &ComposioClient,
    command: &str,
    tools: &[ToolDef],
    mut on_event: impl FnMut(LoopEvent),
    confirm: impl Fn(String, Vec<String>) -> F,
) -> Result<String>
where
    F: Future<Output = bool>,
{
    let schemas: Vec<serde_json::Value> = tools.iter().map(ToolDef::to_openai).collect();
    let mut messages = vec![Message::system(TOOL_SYSTEM_PROMPT), Message::user(command)];

    // Whether any tool call in the run failed — the empty-answer fallback must
    // not say "Done." over a failure.
    let mut any_failed = false;

    for round in 0..MAX_ROUNDS {
        // The last round forbids tools, forcing the model to answer with what it
        // has rather than calling forever.
        let is_last = round == MAX_ROUNDS - 1;
        let chat = || {
            llm.chat_with_tools(
                model,
                &messages,
                (!is_last).then_some(schemas.as_slice()),
                Some(0.3),
            )
        };

        // Ported from `AskService.requestChatMessage`: a 429 mid-loop is
        // retried once after honoring `retry-after` (capped), because the loop
        // makes several calls in a row and a per-minute budget can trip between
        // rounds. The retry lives here, not in the llm crate, so the panel can
        // see the wait instead of looking hung.
        let turn = match chat().await {
            Err(e) => match e.downcast_ref::<LlmError>() {
                Some(LlmError::RateLimited { retry_after, .. }) => {
                    let wait = (*retry_after).min(Duration::from_secs(30));
                    on_event(LoopEvent::RateLimited {
                        seconds: wait.as_secs(),
                    });
                    tokio::time::sleep(wait).await;
                    chat().await?
                }
                _ => return Err(e),
            },
            Ok(turn) => turn,
        };

        if turn.tool_calls.is_empty() {
            return Ok(turn
                .content
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| {
                    if any_failed {
                        "The action failed — see the tool result above.".to_string()
                    } else {
                        "Done.".to_string()
                    }
                }));
        }

        // Record the assistant's tool-call message so the model sees its own
        // request alongside the results we're about to append.
        messages.push(Message {
            role: aloud_llm::Role::Assistant,
            content: turn.content.clone(),
            tool_calls: Some(turn.tool_calls.clone()),
            tool_call_id: None,
        });

        for call in &turn.tool_calls {
            let name = call.function.name.clone();
            let args: serde_json::Value = serde_json::from_str(&call.function.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));
            let arg_lines = arg_display_lines(&args);

            on_event(LoopEvent::ToolCallStart {
                name: name.clone(),
                args: arg_lines.clone(),
            });

            // A write waits for an explicit yes. A rejected write is reported to
            // the model as a normal result so it can respond ("Okay, cancelled")
            // rather than erroring the whole command.
            let result =
                if is_write(&name, tools) && !confirm(name.clone(), arg_lines.clone()).await {
                    on_event(LoopEvent::ToolCallResult {
                        name: name.clone(),
                        success: false,
                    });
                    r#"{"error":"user declined this action"}"#.to_string()
                } else {
                    let result = composio.execute(&name, &args).await;
                    let success = tool_succeeded(&result);
                    any_failed |= !success;
                    on_event(LoopEvent::ToolCallResult {
                        name: name.clone(),
                        success,
                    });
                    result
                };

            messages.push(Message::tool_result(&call.id, result));
        }
    }

    anyhow::bail!("reached the tool-round limit without an answer")
}

/// Whether a tool needs confirmation before it runs.
///
/// Fail-safe by construction: only a tool Composio explicitly marks
/// `readOnlyHint` skips the gate. A create, an update, a delete, an
/// unannotated tool, or a name that is not in the offered set at all is
/// treated as a write. This replaced a hardcoded list of two slugs, which
/// silently let every tool of a newly connected toolkit run unconfirmed.
fn is_write(name: &str, tools: &[ToolDef]) -> bool {
    !tools
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.read_only)
        .unwrap_or(false)
}

/// Whether a compacted tool result reports success. The execute path returns
/// `{"successful": bool, …}` on a real call and `{"error": …}` (no `successful`)
/// on a transport failure — so trust `successful` when present, else treat the
/// presence of a non-null `error` as failure.
fn tool_succeeded(result: &str) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(result) else {
        return false;
    };
    // Composio relays the upstream API's status inside `data` (observed live:
    // a 404 repo carries data.status_code=404) — an error status is a failure
    // even if the wrapper's `successful` flag disagrees.
    if let Some(code) = json
        .pointer("/data/status_code")
        .and_then(serde_json::Value::as_u64)
    {
        if code >= 400 {
            return false;
        }
    }
    if let Some(ok) = json.get("successful").and_then(serde_json::Value::as_bool) {
        return ok;
    }
    !matches!(json.get("error"), Some(e) if !e.is_null())
}

/// Turns a JSON args object into "field: value" display lines for the panel and
/// the confirmation prompt. Non-objects render as a single line.
fn arg_display_lines(args: &serde_json::Value) -> Vec<String> {
    match args.as_object() {
        Some(map) => map
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}: {value}")
            })
            .collect(),
        None => vec![args.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, read_only: bool) -> ToolDef {
        ToolDef {
            name: name.into(),
            description: String::new(),
            parameters: serde_json::json!({}),
            read_only,
        }
    }

    /// The confirmation gate is the one non-negotiable safety property, so it
    /// must fail closed: anything not proven read-only asks first.
    #[test]
    fn only_explicitly_read_only_tools_skip_confirmation() {
        let tools = [
            tool("GMAIL_FETCH_EMAILS", true),
            tool("GMAIL_SEND_EMAIL", false),
        ];

        assert!(!is_write("GMAIL_FETCH_EMAILS", &tools));
        assert!(is_write("GMAIL_SEND_EMAIL", &tools));
        // A hallucinated or newly added name we were never offered must not
        // slip through as a read.
        assert!(is_write("SLACK_DELETE_EVERYTHING", &tools));
        assert!(is_write("anything", &[]));
    }

    #[test]
    fn success_trusts_the_successful_flag_not_a_null_error() {
        assert!(tool_succeeded(
            r#"{"successful":true,"error":null,"data":{}}"#
        ));
        assert!(!tool_succeeded(r#"{"successful":false,"data":{}}"#));
        // transport failures have no `successful`, just an error string
        assert!(!tool_succeeded(r#"{"error":"no active gmail connection"}"#));
        assert!(!tool_succeeded("not json"));
    }

    #[test]
    fn upstream_http_error_in_data_is_a_failure_even_if_successful_says_true() {
        // Observed live: a nonexistent repo comes back with the GitHub 404
        // relayed in data.status_code. Trust that over the wrapper's flag.
        assert!(!tool_succeeded(
            r#"{"successful":true,"data":{"status_code":404,"message":"Not Found"}}"#
        ));
        assert!(tool_succeeded(
            r#"{"successful":true,"data":{"status_code":201,"number":7}}"#
        ));
    }

    #[test]
    fn arg_lines_render_strings_without_quotes() {
        let args = serde_json::json!({ "to": "a@b.com", "max_results": 4 });
        let lines = arg_display_lines(&args);
        assert!(lines.contains(&"to: a@b.com".to_string()));
        assert!(lines.contains(&"max_results: 4".to_string()));
    }
}
