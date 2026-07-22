//! The Composio tool loop: model ↔ tools until the model produces a final
//! answer.
//!
//! Bounded rounds — the last forbids tools, so the model must answer rather than
//! loop forever. Write tools pause for an explicit confirmation before they run;
//! that is the one non-negotiable gate.

use crate::composio::{ComposioClient, ToolDef};
use aloud_llm::{LlmClient, LlmError, Message};
use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::Instrument;

const MAX_ROUNDS: usize = 5;

/// How many times one read tool may run in a single command. Rounds are already
/// capped, but a round can hold any number of calls, and a model that varies an
/// argument slips past the identical-call cache indefinitely.
const MAX_CALLS_PER_TOOL: usize = 3;

/// How many questions one command may put to the user. See the ask tool below.
const MAX_QUESTIONS: usize = 2;

/// The one tool this crate owns rather than Composio.
///
/// A model that needs something only the user has — an address no directory
/// held — will otherwise say so in its final answer, which arrives after the
/// run is over and lands in a panel with nothing to type into. Given a tool, it
/// asks *during* the run and the loop suspends for the reply.
///
/// A synthetic tool rather than a new wire protocol because the model already
/// knows how to call tools, and this works on any OpenAI-compatible provider.
/// The prefix is ours: every Composio slug is `TOOLKIT_VERB` for a real
/// toolkit, so nothing can collide with it.
pub const ASK_USER: &str = "ALOUD_ASK_USER";

fn ask_user_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": ASK_USER,
            "description":
                "Ask the user something and wait for their reply. The run pauses \
                 until they answer, and their answer comes back as this tool's \
                 result. Ask one thing at a time, in one short sentence.",
            "parameters": {
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question, as one short sentence.",
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description":
                            "The answers to offer, when the answer is one of a known \
                             set — the people a search returned, the calendars an \
                             event could go on. The user picks one rather than \
                             typing. Give each as the full value you need back, not \
                             a label: an email address, not \"the first one\". Omit \
                             entirely when the answer is not from a known set.",
                    },
                },
                "required": ["question"],
            },
        }
    })
}

/// Progress events for the panel, in AG-UI tool-call vocabulary. The caller
/// maps these onto its own UI events.
#[derive(Debug, Clone)]
pub enum LoopEvent {
    /// Every tool this round intends to run, before the first starts — for an
    /// agent with write access to somebody's mail, "it is about to do three
    /// things, one of which sends" is worth reading first. Names only; the
    /// arguments arrive with `ToolCallStart`, where a plan becomes an action.
    Planned { names: Vec<String> },
    /// The model asked to run a tool, with the arguments it chose. The whole
    /// JSON object, not a flattened list: the panel renders a draft's body as
    /// text and ignores `user_id`, which needs the fields kept apart.
    ToolCallStart {
        name: String,
        args: serde_json::Value,
    },
    /// That tool finished (success carries no payload; the model gets the JSON).
    ToolCallResult { name: String, success: bool },
    /// The model needs something only the user has; the loop is suspended until
    /// the panel answers. `options` non-empty means the answer is one of them —
    /// a choice, not free text. See [`ASK_USER`].
    Asked {
        question: String,
        options: Vec<String>,
    },
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
You can ask the user a question mid-run with the ask tool, and the run waits \
for their reply. Ask when a value is genuinely required, no tool can supply \
it, and getting it wrong would be worse than the interruption — a recipient, a \
date the command left ambiguous, which of several matches was meant. When the \
answer is one of a known set, pass those as options so they pick rather than \
type. \
Do not ask for anything a tool could tell you, anything the command already \
implies, or a detail with a sensible default — a subject line, a meeting \
length, which calendar. Asking is expensive: this is a voice app, the panel is \
small, and two questions for one command is worse than a reasonable \
assumption stated plainly in your answer. Prefer acting and saying what you \
assumed. Never ask twice for the same thing, and never ask in your final \
answer instead of with the tool — that answer arrives after the run is over \
and cannot be replied to. \
The user speaks names, not addresses. When an argument needs an email address, \
a Slack user or a GitHub username and the command gives only a name, look it up \
first with the directory tool in the list and use what it returns — never \
invent an address, and never ask the user to spell one out. \
A result only counts as the person if their name actually matches the name you \
were given: the same person, allowing for spelling, capitalisation, a middle \
name or a missing surname. These tools return their best guesses, not matches, \
so a query for one person routinely comes back with somebody unrelated. Never \
take an unrelated person because they were the only result, the first result, \
or the closest of several — a wrong recipient is worse than no recipient, and \
the user cannot tell you picked one. If nothing matches the name, that lookup \
found nobody; treat it exactly as an empty result. If several do match, do not \
choose between them: ask, with their addresses as the options. \
When a lookup finds nobody, the person may still have been corresponded with: \
search the mail itself for the name next, and take the address off a message \
header — again only if that header is the person you were asked about. \
If you did resolve an address, save the person to the user's contacts \
afterwards so the next command finds them straight away, using the name you \
were given and the address you resolved. \
If both lookups found nobody, do not send anything and do not guess: ask for \
the address, then use it for what they originally asked for and save the \
contact as well. Never put another person's name or address in a call to fill \
the gap, and never fall back to sending to whoever a search did return. \
Fill every argument you send with real content. If the user asks you to write \
or draft something, compose the actual text — never send an empty body, an \
empty subject, or a placeholder. If the same content is needed by two calls, \
repeat it in full rather than leaving the second one blank. Omit optional \
arguments you have no real value for — use the schema's default rather than \
inventing identifiers.";

/// The model has no clock: without this, "tomorrow at 4pm" landed on
/// 2023-07-20T09:00-07:00 — right time of day, wrong year, stamped UTC. Naming
/// the zone alone is not enough either; told only "(Asia/Calcutta)" it still
/// searched `T17:00:00Z` for a 5pm event. The literal offset is what stops it.
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
/// `confirm` is awaited before any write tool runs. It resolves to `None` when
/// the user declines, or to the arguments to run with — which are not
/// necessarily the ones passed in, because the gate lets a field be filled in
/// or corrected before approving.
// ponytail: eight arguments, five of them the run's inputs and three its
// callbacks. A `Callbacks { on_event, confirm, ask }` struct is the fix if a
// fourth ever appears; at three it would be ceremony around the same tuple.
#[allow(clippy::too_many_arguments)]
pub async fn run<F, A>(
    llm: &LlmClient,
    model: &str,
    composio: &ComposioClient,
    command: &str,
    tools: &[ToolDef],
    mut on_event: impl FnMut(LoopEvent),
    confirm: impl Fn(String, serde_json::Value) -> F,
    ask: impl Fn(String) -> A,
) -> Result<String>
where
    F: Future<Output = Option<serde_json::Value>>,
    A: Future<Output = Option<String>>,
{
    // The synthetic tool rides alongside Composio's; the model cannot tell them
    // apart, and `execute` never sees it.
    let mut schemas: Vec<serde_json::Value> = tools.iter().map(ToolDef::to_openai).collect();
    schemas.push(ask_user_schema());
    let mut messages = vec![
        Message::system(format!("{TOOL_SYSTEM_PROMPT} {}", now_line())),
        Message::user(command),
    ];

    // The empty-answer fallback must not say "Done." over a failure.
    let mut any_failed = false;
    // Reads already made this run, keyed by tool and exact arguments, and how
    // many times each tool has run. Both are per-run: a fresh command asks
    // fresh questions.
    let mut seen: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    let mut budget: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut asked = 0usize;

    for round in 0..MAX_ROUNDS {
        let is_last = round == MAX_ROUNDS - 1;
        let chat = || {
            llm.chat_with_tools(
                model,
                &messages,
                (!is_last).then_some(schemas.as_slice()),
                Some(0.3),
            )
            // Per round, not per run: "the model was slow" and "the model went
            // round twice" are different problems with different fixes.
            .instrument(tracing::info_span!(
                "tool_select",
                model,
                round,
                usage = tracing::field::Empty
            ))
        };

        // A 429 mid-loop is retried once after a capped `retry-after`: the loop
        // makes several calls in a row and a per-minute budget can trip between
        // rounds. Here rather than in the llm crate so the panel can show the
        // wait instead of looking hung.
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

        // The model must see its own request alongside the results appended
        // below, or it cannot match them up.
        messages.push(Message {
            role: aloud_llm::Role::Assistant,
            content: turn.content.clone(),
            tool_calls: Some(turn.tool_calls.clone()),
            tool_call_id: None,
        });

        // A queue of one is a second surface saying what the step list says.
        if turn.tool_calls.len() > 1 {
            on_event(LoopEvent::Planned {
                names: turn
                    .tool_calls
                    .iter()
                    .map(|c| c.function.name.clone())
                    .collect(),
            });
        }

        for call in &turn.tool_calls {
            let name = call.function.name.clone();
            let args: serde_json::Value = serde_json::from_str(&call.function.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));

            // Answered by the user, not by Composio. Before the dedup below,
            // which would hand back a previous answer to a new question.
            if name == ASK_USER {
                let question = args["question"].as_str().unwrap_or_default().to_string();
                let options: Vec<String> = args["options"]
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(|o| o.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                // An agent that can ask can interrogate. Two is the most a
                // spoken command should ever cost; past that the model is
                // gathering requirements, and the honest move is to act on what
                // it has and say what it assumed.
                asked += 1;
                if asked > MAX_QUESTIONS {
                    tracing::warn!("question budget exhausted");
                    messages.push(Message::tool_result(
                        &call.id,
                        r#"{"error":"no more questions can be asked this run. Act on what you have and state any assumption in your answer."}"#,
                    ));
                    continue;
                }
                on_event(LoopEvent::Asked {
                    question: question.clone(),
                    options,
                });
                let answer = ask(question)
                    .instrument(tracing::info_span!("ask_user"))
                    .await;
                let result = match answer {
                    Some(text) => serde_json::json!({ "answer": text }).to_string(),
                    // Dismissed. Reported as a result rather than an error so the
                    // model closes the run politely instead of retrying.
                    None => r#"{"error":"the user did not answer"}"#.to_string(),
                };
                on_event(LoopEvent::ToolCallResult {
                    name: name.clone(),
                    success: true,
                });
                messages.push(Message::tool_result(&call.id, result));
                continue;
            }

            // A read that has already run this turn is answered from the first
            // result instead of being repeated. Looking one person up cost
            // three round-trips — the same inbox query twice, then a third — and
            // the model has no memory of having asked. Writes are excluded: two
            // identical sends are two emails, and collapsing them would swallow
            // the second silently.
            if !is_write(&name, tools) {
                if let Some(previous) = seen.get(&(name.clone(), args.to_string())) {
                    tracing::debug!(tool = %name, "identical read already made; reusing");
                    messages.push(Message::tool_result(&call.id, previous.clone()));
                    continue;
                }
                // Independent of the dedup above: a model that varies one
                // argument each time never hits the cache and would otherwise
                // loop until MAX_ROUNDS.
                let calls = budget.entry(name.clone()).or_insert(0);
                *calls += 1;
                if *calls > MAX_CALLS_PER_TOOL {
                    tracing::warn!(tool = %name, "per-tool budget exhausted");
                    messages.push(Message::tool_result(
                        &call.id,
                        format!(
                            "{{\"error\":\"{name} has already been called \
                             {MAX_CALLS_PER_TOOL} times this run and will not run \
                             again. Answer from what you have.\"}}"
                        ),
                    ));
                    continue;
                }
            }

            on_event(LoopEvent::ToolCallStart {
                name: name.clone(),
                args: args.clone(),
            });

            // The toolkit drives the latency profile — a Gmail send and a
            // Calendar create behave nothing alike — so it is a span field
            // rather than something to infer from the tool name.
            let toolkit = tools
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.toolkit.as_str())
                .unwrap_or("unknown");

            // Confirmation is the user thinking, not the app working: its own
            // span so a 40s wait is not charged to pipeline latency. A declined
            // write returns as a normal result so the model can say "cancelled"
            // rather than erroring the whole command.
            // Rebound, not just checked: the gate returns the arguments to run
            // with, so a field the user filled in at the gate is what executes.
            // A read tool never reaches the gate and keeps the model's.
            let mut args = args;
            let declined = if is_write(&name, tools) {
                match confirm(name.clone(), args.clone())
                    .instrument(tracing::info_span!("confirm", tool = %name, toolkit))
                    .await
                {
                    Some(approved) => {
                        args = approved;
                        false
                    }
                    None => true,
                }
            } else {
                false
            };

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

            if !declined && !is_write(&name, tools) {
                seen.insert((name.clone(), args.to_string()), result.clone());
            }
            messages.push(Message::tool_result(&call.id, result));
        }
    }

    anyhow::bail!("reached the tool-round limit without an answer")
}

/// Whether a tool needs confirmation. Fail-safe: only an explicit
/// `readOnlyHint` skips the gate, so an unannotated tool — or a name never
/// offered at all — counts as a write.
fn is_write(name: &str, tools: &[ToolDef]) -> bool {
    !tools
        .iter()
        .find(|t| t.name == name)
        .map(|t| t.read_only)
        .unwrap_or(false)
}

/// Whether a compacted tool result reports success: trust `successful` when
/// present, else treat a non-null `error` (a transport failure) as failure.
fn tool_succeeded(result: &str) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(result) else {
        return false;
    };
    // Composio relays the upstream status inside `data`, and an error there is
    // a failure even when the wrapper's `successful` flag disagrees.
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

    #[test]
    fn the_prompt_carries_a_current_year_not_the_models_training_year() {
        let line = now_line();
        let year = chrono::Local::now().format("%Y").to_string();
        assert!(line.contains(&year), "got: {line}");
    }

    /// Naming the zone alone left the model sending `...T17:00:00Z` for 5pm local.
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
        // A hallucinated name we never offered must not pass as a read.
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
        // Observed live: a nonexistent repo relays GitHub's 404 in
        // data.status_code. Trust that over the wrapper's flag.
        assert!(!tool_succeeded(
            r#"{"successful":true,"data":{"status_code":404,"message":"Not Found"}}"#
        ));
        assert!(tool_succeeded(
            r#"{"successful":true,"data":{"status_code":201,"number":7}}"#
        ));
    }
}
