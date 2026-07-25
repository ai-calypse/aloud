//! The Composio tool loop: model ↔ tools until the model produces a final
//! answer.
//!
//! Bounded rounds — the last forbids tools, so the model must answer rather than
//! loop forever. Write tools pause for an explicit confirmation before they run;
//! that is the one non-negotiable gate.
//!
//! The model stopping is not the same as the model being done: asked to draft
//! *and* send, it will draft, announce success and fall silent. So a stop is
//! graded against the original command before it is accepted — see [`verify`].

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

/// How many times the verifier may send the model back for an unfinished
/// sub-goal. Two: one nudge covers "drafted it but never sent it", and a model
/// still unfinished after two is looping rather than working.
const MAX_VERIFIER_RETRIES: usize = 2;

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
Finish the whole command before you stop. A command that names two actions \
(\"draft it and send it\", \"file the issue and assign it to me\") is not done \
when the first one is — carry out every part, and only stop once each has \
actually been performed by a tool call that succeeded. Do not describe an \
action you have not taken as though you have taken it. \
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

/// 0…0.4s of jitter, off the nanosecond clock rather than a `rand` dependency
/// added for four bytes. Not uniform and not worth being: the only requirement
/// is that two clients retrying the same limit do not wake together.
fn jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos % 400) / 1000.0
}

/// The backoff schedule for an error, or `None` if it is not worth retrying.
/// Attempt `n` sleeps `schedule[n]` before retrying, so the length is the retry
/// count. Overload (429/529) backs off hard because the limit is a budget that
/// needs time to refill; a 5xx is usually one bad box behind a load balancer,
/// where a near-immediate retry lands on a different one.
fn backoff(error: &LlmError) -> Option<&'static [f64]> {
    const OVERLOADED: &[f64] = &[1.0, 4.0, 16.0];
    const TRANSIENT: &[f64] = &[0.25, 1.0];
    match error {
        LlmError::RateLimited { .. } => Some(OVERLOADED),
        LlmError::Failed { status: 529, .. } => Some(OVERLOADED),
        LlmError::Failed {
            status: 500 | 502 | 503 | 504,
            ..
        } => Some(TRANSIENT),
        _ => None,
    }
}

/// How long to wait before retry attempt `attempt`. A server-sent `retry-after`
/// outranks the schedule — it knows the real budget — but is capped so a
/// "come back in an hour" cannot strand a spoken command; past the cap we would
/// rather fail and tell the user than hold the panel open.
///
/// The jitter matters more here than it looks: the daily-limit path retries
/// every dictation at the same schedule, so without it a burst of commands
/// syncs up and re-hits the limit together.
fn retry_delay(error: &LlmError, schedule: &[f64], attempt: usize, jitter: f64) -> Duration {
    const CAP: Duration = Duration::from_secs(30);
    if let LlmError::RateLimited { retry_after, .. } = error {
        return (*retry_after).min(CAP);
    }
    Duration::from_secs_f64(schedule[attempt] + jitter).min(CAP)
}

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

const VERIFIER_PROMPT: &str = "\
You are grading whether an assistant finished a spoken command. You are given \
the command and the assistant's final answer. Decide only whether every action \
the command asked for was actually carried out — not whether it was done well. \
An answer that stops because the assistant asked the user something and got \
no reply is FINISHED: it is waiting on a person, and nothing it does next can \
change that. A command naming two actions is unfinished if the answer only \
evidences one \
(asked to draft and send, the answer says a draft was created). An answer \
saying an action failed is FINISHED: the failure is reported honestly and the \
assistant cannot do more. An answer that simply refuses, or that puts a \
question to the user in the answer itself rather than having asked during the \
run, is UNFINISHED. \
Reply with exactly one word, FINISHED or UNFINISHED, then — only \
if UNFINISHED — a colon and a short phrase naming what is still undone.";

/// Whether the model's final answer actually completed `command`, and if not,
/// what is missing. One cheap call, no tools.
///
/// Fails open. A grader that errors, times out or answers in a shape we do not
/// recognise returns `None` and the answer stands — the failure mode of a
/// broken grader must be an unverified answer, never a command trapped in a
/// loop it cannot leave. This is why the check is "does it start with
/// UNFINISHED" rather than "is it FINISHED": every unexpected output accepts.
async fn verify(llm: &LlmClient, model: &str, command: &str, answer: &str) -> Option<String> {
    let messages = [
        Message::system(VERIFIER_PROMPT),
        Message::user(format!("COMMAND: {command}\n\nFINAL ANSWER: {answer}")),
    ];
    let turn = llm
        .chat_with_tools(model, &messages, None, Some(0.0))
        .instrument(tracing::info_span!("verify", model))
        .await
        .map_err(|e| tracing::warn!(%e, "completion verifier failed; accepting the answer"))
        .ok()?;

    unfinished_reason(&turn.content?)
}

/// Parses the grader's verdict into "what is still undone", or `None` to
/// accept the answer. Split out from [`verify`] because the fail-open
/// direction is the whole safety property and deserves a test without a live
/// model behind it.
fn unfinished_reason(verdict: &str) -> Option<String> {
    let rest = verdict.trim().strip_prefix("UNFINISHED")?;
    Some(
        rest.trim_start_matches([':', '.', ' ', '\n'])
            .trim()
            .to_string(),
    )
}

/// Runs the loop for `command`. Returns the model's final answer.
///
/// `on_event` fires for each tool call so the panel can show progress.
/// `confirm` is awaited before any write tool runs. It resolves to `None` when
/// the user declines, or to the arguments to run with — which are not
/// necessarily the ones passed in, because the gate lets a field be filled in
/// or corrected before approving.
///
/// `cancelled` is polled at the top of each round and between individual tool
/// dispatches, so Escape during a six-tool round stops after the tool in
/// flight rather than running the other five.
// ponytail: nine arguments, five of them the run's inputs and four its
// callbacks — the fourth the previous comment here said would justify a
// `Callbacks { on_event, confirm, ask, cancelled }` struct. Left as-is because
// there is still exactly one real construction site (the Tauri command; the
// example is the other) and a struct would be ceremony around the same tuple.
// Worth doing the moment a second caller has to assemble this list.
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
    cancelled: impl Fn() -> bool,
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
    let mut verifier_retries = 0;

    for round in 0..MAX_ROUNDS {
        if cancelled() {
            anyhow::bail!("cancelled");
        }
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

        // Per round, not per run: a run that survived a 429 in round one still
        // deserves a full ladder if round three trips a different limit.
        let mut attempts = 0usize;

        // Overload and transient 5xx are retried on a backoff ladder: the loop
        // makes several calls in a row, so a per-minute budget can trip between
        // rounds and a single bad upstream box can end a command that was
        // otherwise going fine. Here rather than in the llm crate so the panel
        // can show the wait instead of looking hung.
        let turn = loop {
            let error = match chat().await {
                Ok(turn) => break turn,
                Err(e) => e,
            };
            let Some(schedule) = error.downcast_ref::<LlmError>().and_then(backoff) else {
                return Err(error);
            };
            let attempt = attempts;
            if attempt >= schedule.len() {
                return Err(error);
            }
            attempts += 1;

            let llm_error = error.downcast_ref::<LlmError>().expect("matched above");
            let wait = retry_delay(llm_error, schedule, attempt, jitter());
            // The panel says "rate limited" for every retried class: to someone
            // waiting on a spoken command, a 503 and a 429 are the same event —
            // the machine is busy, it will try again. The real status is logged.
            tracing::warn!(%error, attempt, wait_s = wait.as_secs_f64(), "retrying chat");
            on_event(LoopEvent::RateLimited {
                seconds: wait.as_secs().max(1),
            });
            async { tokio::time::sleep(wait).await }
                .instrument(tracing::info_span!(
                    "tool_select_retry",
                    round,
                    attempt,
                    waited_s = wait.as_secs_f64()
                ))
                .await;
            if cancelled() {
                anyhow::bail!("cancelled");
            }
        };

        if turn.tool_calls.is_empty() {
            let answer = turn
                .content
                .clone()
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| {
                    if any_failed {
                        "The action failed — see the tool result above.".to_string()
                    } else {
                        "Done.".to_string()
                    }
                });

            // The model stopping is only a claim to be done. Grade it against
            // the original command, and if a named action never happened, put
            // that back to the model rather than telling the user it is done.
            // Skipped once the budget is spent and on the last round, where
            // tools are forbidden and a nudge could not be acted on anyway.
            if verifier_retries < MAX_VERIFIER_RETRIES && !is_last && !cancelled() {
                if let Some(missing) = verify(llm, model, command, &answer).await {
                    verifier_retries += 1;
                    tracing::info!(%missing, verifier_retries, "answer graded unfinished");
                    messages.push(Message {
                        role: aloud_llm::Role::Assistant,
                        content: Some(answer),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                    messages.push(Message::user(format!(
                        "You have not finished the command yet: {missing}. \
                         Do it now with the tools available, then confirm. \
                         If it genuinely cannot be done, say plainly why."
                    )));
                    continue;
                }
            }

            return Ok(answer);
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
            // Between dispatches, not just at the top of the round: a round
            // holding six calls otherwise runs all six after Escape, and the
            // ones that matter — the writes — tend to come last.
            if cancelled() {
                anyhow::bail!("cancelled");
            }

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
                // Escape while the question was on screen. Checked here as well
                // as at the top of the round because this is the one await that
                // can hold a run open indefinitely — the user is being waited
                // on, and dismissing the panel is how they say no.
                if cancelled() {
                    anyhow::bail!("cancelled");
                }
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
                    "tool_exec",
                    tool = %name,
                    toolkit,
                    ok = tracing::field::Empty
                );
                // Native macOS tools are ours; a Composio slug is not, so
                // `aloud_native::execute` returns None and we fall through. This
                // is the whole second execution path — one branch, because the
                // gate, dedup, and budgets above all key off the tool name.
                // osascript blocks, so it runs on a blocking thread rather than
                // stalling the runtime.
                let result = match run_native(&name, &args).instrument(span.clone()).await {
                    Some(native) => native,
                    None => composio.execute(&name, &args).instrument(span.clone()).await,
                };
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

/// Runs a native macOS tool, returning `Some(json_string)` in the same shape
/// Composio's `execute` produces — or `None` when the name is not a native
/// tool, the fall-through signal. osascript blocks, so it runs on a blocking
/// thread; a permission denial comes back as a readable `{"error":…}` the model
/// can relay rather than an `Err` that kills the run.
async fn run_native(name: &str, args: &serde_json::Value) -> Option<String> {
    let name = name.to_string();
    let args = args.clone();
    // spawn_blocking so the osascript subprocess wait does not stall the
    // executor. The JoinError case (a panic in the closure) is reported as a
    // native tool result, not a fall-through — the name *was* ours.
    let outcome = tokio::task::spawn_blocking(move || aloud_native::execute(&name, &args))
        .await
        .unwrap_or_else(|e| Some(Err(aloud_native::ScriptError::Failed(e.to_string()))))?;
    Some(match outcome {
        Ok(value) => value.to_string(),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    })
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

    /// The verifier must fail open. A grader that answers in a shape we did
    /// not anticipate has to accept the answer, because the alternative is a
    /// spoken command that cannot leave the loop.
    #[test]
    fn the_verifier_only_reopens_on_an_explicit_unfinished() {
        assert_eq!(
            unfinished_reason("UNFINISHED: the email was drafted but never sent"),
            Some("the email was drafted but never sent".to_string())
        );
        assert_eq!(
            unfinished_reason("  UNFINISHED — never sent\n"),
            Some("— never sent".to_string())
        );
        // A bare UNFINISHED still reopens; the nudge just carries no detail.
        assert_eq!(unfinished_reason("UNFINISHED"), Some(String::new()));

        assert_eq!(unfinished_reason("FINISHED"), None);
        assert_eq!(unfinished_reason(""), None);
        // Everything unrecognised accepts, including a grader that editorialises
        // or answers in the wrong case rather than the one word it was asked for.
        assert_eq!(unfinished_reason("I think it looks unfinished?"), None);
        assert_eq!(unfinished_reason("unfinished: lowercase"), None);
        assert_eq!(unfinished_reason("The answer is UNFINISHED"), None);
    }

    #[test]
    fn only_overload_and_transient_server_errors_are_retried() {
        let rate_limited = LlmError::RateLimited {
            model: "m".into(),
            retry_after: Duration::from_secs(2),
        };
        assert_eq!(backoff(&rate_limited).map(<[f64]>::len), Some(3));
        assert_eq!(
            backoff(&LlmError::Failed {
                status: 529,
                body: String::new()
            })
            .map(<[f64]>::len),
            Some(3)
        );
        assert_eq!(
            backoff(&LlmError::Failed {
                status: 503,
                body: String::new()
            })
            .map(<[f64]>::len),
            Some(2)
        );
        // A bad request or a bad key will fail identically forever; retrying
        // one only makes the user wait longer for the same error.
        for status in [400, 401, 403, 404] {
            assert!(
                backoff(&LlmError::Failed {
                    status,
                    body: String::new()
                })
                .is_none(),
                "{status} should not be retried"
            );
        }
        assert!(backoff(&LlmError::EmptyOutput).is_none());
    }

    /// A server that says "come back in an hour" must not hold a spoken
    /// command open for an hour.
    #[test]
    fn retry_after_outranks_the_schedule_but_is_capped() {
        let schedule = backoff(&LlmError::RateLimited {
            model: "m".into(),
            retry_after: Duration::ZERO,
        })
        .unwrap();

        let polite = LlmError::RateLimited {
            model: "m".into(),
            retry_after: Duration::from_secs(5),
        };
        assert_eq!(
            retry_delay(&polite, schedule, 0, 0.0),
            Duration::from_secs(5)
        );

        let hostile = LlmError::RateLimited {
            model: "m".into(),
            retry_after: Duration::from_secs(3600),
        };
        assert_eq!(
            retry_delay(&hostile, schedule, 0, 0.0),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn the_5xx_ladder_grows_and_stays_under_the_cap() {
        let err = LlmError::Failed {
            status: 500,
            body: String::new(),
        };
        let schedule = backoff(&err).unwrap();
        let first = retry_delay(&err, schedule, 0, 0.0);
        let second = retry_delay(&err, schedule, 1, 0.0);
        assert!(
            first < second,
            "{first:?} should be shorter than {second:?}"
        );
        assert!(second <= Duration::from_secs(30));
        // Jitter is additive, and must not push a rung over the cap either.
        assert!(retry_delay(&err, schedule, 1, 0.4) <= Duration::from_secs(30));
    }

    #[test]
    fn jitter_stays_inside_its_advertised_range() {
        for _ in 0..100 {
            let j = jitter();
            assert!((0.0..0.4).contains(&j), "got {j}");
        }
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
