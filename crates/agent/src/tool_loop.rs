//! The Composio tool loop: model ↔ tools until the model produces a final
//! answer. A direct port of voice-dict's `AskService.answerUsingTools`.
//!
//! The model is told which tools its connected apps offer; it either answers or asks
//! to call tools; each call executes through Composio and the result is fed
//! back. Bounded rounds — the last round forbids tools so the model must answer
//! rather than loop forever. Write tools (send email, create issue) pause for an
//! explicit confirmation before they run; that is the one non-negotiable gate.

use crate::composio::{ComposioClient, ToolDef};
use aloud_llm::{LlmClient, LlmError, Message};
use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::Instrument;

const MAX_ROUNDS: usize = 5;

/// Progress events for the panel, in AG-UI tool-call vocabulary. The caller
/// maps these onto its own UI events.
#[derive(Debug, Clone)]
pub enum LoopEvent {
    /// The model asked to run a tool, with the arguments it chose.
    ///
    /// The whole JSON object, not a flattened list: the panel renders a draft's
    /// body as text and ignores `user_id`, which it can only do if it still has
    /// the fields to choose between.
    ToolCallStart {
        name: String,
        args: serde_json::Value,
    },
    /// That tool finished (success carries no payload; the model gets the JSON).
    ToolCallResult { name: String, success: bool },
    /// The chat API throttled us; the loop is waiting this long to retry once.
    RateLimited { seconds: u64 },
}

const TOOL_SYSTEM_PROMPT: &str = "\
You are a concise assistant acting on a spoken command. You can call tools to \
act on the user's connected apps (email, calendar, GitHub, and whatever else \
the tool list offers). Only call a tool when the command asks for such an \
action or needs that data; otherwise just answer. \
The command is a transcription of speech and may contain minor errors; act on \
the intent. After acting, confirm what you did in one short plain sentence. \
If a tool result was not successful or contains an error, say the action \
failed and why — never claim it worked. But if it failed because an argument \
was invalid, fix that argument and retry once before reporting failure. \
Fill every argument you send with real content. If the user asks you to write \
or draft something, compose the actual text — never send an empty body, an \
empty subject, or a placeholder. If the same content is needed by two calls, \
repeat it in full rather than leaving the second one blank. Omit optional \
arguments you have no real value for — use the schema's default rather than \
inventing identifiers.";

/// The model has no clock, so "tomorrow at 4pm" resolves against its training
/// data: a "4 to 6pm" event landed on 2023-07-20T09:00-07:00 — right time of
/// day, wrong year, and stamped UTC instead of the user's zone. Every relative
/// date and every timezone argument depends on this line.
///
/// Naming the zone is not enough on its own. Told only "(Asia/Calcutta)", the
/// model still searched `2026-07-18T17:00:00Z` for a 5pm event — 10:30pm local
/// — found an empty window, and burned four lookups widening it. Spelling the
/// offset out, in the exact format the arguments want, is what stops that.
fn now_line() -> String {
    let now = chrono::Local::now();
    let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| now.offset().to_string());
    format!(
        "The current date and time is {} ({zone}), which in ISO-8601 is {}. \
         Resolve every relative date (\"today\", \"tomorrow\", \"next Friday\") \
         against it. Every timestamp you send must carry this same {} offset — \
         never a \"Z\" suffix, never UTC, unless the user names another zone.",
        now.format("%A %-d %B %Y, %H:%M"),
        now.format("%Y-%m-%dT%H:%M:%S%:z"),
        now.format("%:z"),
    )
}

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
    confirm: impl Fn(String, serde_json::Value) -> F,
) -> Result<String>
where
    F: Future<Output = bool>,
{
    let schemas: Vec<serde_json::Value> = tools.iter().map(ToolDef::to_openai).collect();
    let mut messages = vec![
        Message::system(format!("{TOOL_SYSTEM_PROMPT} {}", now_line())),
        Message::user(command),
    ];

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
            // Per round, not per run: a command that takes three rounds to
            // settle spends three model calls, and "the model was slow" and
            // "the model went round twice" are different problems with
            // different fixes.
            .instrument(tracing::info_span!("tool_select", model, round))
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
                    async {
                        tokio::time::sleep(wait).await;
                        chat().await
                    }
                    .instrument(tracing::info_span!(
                        "tool_select_retry",
                        round,
                        waited_s = wait.as_secs()
                    ))
                    .await?
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

            on_event(LoopEvent::ToolCallStart {
                name: name.clone(),
                args: args.clone(),
            });

            // A write waits for an explicit yes. A rejected write is reported to
            // the model as a normal result so it can respond ("Okay, cancelled")
            // rather than erroring the whole command.
            // The toolkit drives the latency profile — a Gmail send and a
            // Calendar create behave nothing alike — so it is a field on the
            // exec span rather than something to infer from the tool name.
            let toolkit = tools
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.toolkit.as_str())
                .unwrap_or("unknown");

            // Confirmation is the user thinking, not the app working. Its own
            // span so it can be subtracted: left inside the run it would put a
            // 40s "latency" spike on a pipeline that was never running.
            let declined = is_write(&name, tools)
                && !confirm(name.clone(), args.clone())
                    .instrument(tracing::info_span!("confirm", tool = %name, toolkit))
                    .await;

            let result = if declined {
                on_event(LoopEvent::ToolCallResult {
                    name: name.clone(),
                    success: false,
                });
                r#"{"error":"user declined this action"}"#.to_string()
            } else {
                let span = tracing::info_span!(
                    "composio_exec",
                    tool = %name,
                    toolkit,
                    ok = tracing::field::Empty
                );
                let result = composio
                    .execute(&name, &args)
                    .instrument(span.clone())
                    .await;
                let success = tool_succeeded(&result);
                span.record("ok", success);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, read_only: bool) -> ToolDef {
        ToolDef {
            name: name.into(),
            toolkit: crate::composio::toolkit_of_slug(name),
            description: String::new(),
            parameters: serde_json::json!({}),
            read_only,
        }
    }

    /// The confirmation gate is the one non-negotiable safety property, so it
    /// must fail closed: anything not proven read-only asks first.
    #[test]
    fn the_prompt_carries_a_current_year_not_the_models_training_year() {
        let line = now_line();
        let year = chrono::Local::now().format("%Y").to_string();
        assert!(line.contains(&year), "got: {line}");
    }

    /// Naming the zone alone left the model sending `...T17:00:00Z` for 5pm
    /// local. The literal offset is the part that stops it.
    #[test]
    fn the_prompt_spells_out_the_utc_offset() {
        let line = now_line();
        let offset = chrono::Local::now().format("%:z").to_string();
        assert!(
            line.contains(&offset),
            "offset {offset} missing from: {line}"
        );
        assert!(line.contains("never a \"Z\" suffix"), "got: {line}");
    }

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
}
