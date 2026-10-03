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
use crate::Brain;
use aloud_llm::{LlmError, Message};
use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::Instrument;

/// Rounds allowed a command that only calls structured tools. Five covers the
/// shape those have — look something up, act on it, confirm — and a model still
/// going after five is looping rather than working.
const MAX_ROUNDS: usize = 5;

/// Rounds allowed once a command starts driving the screen.
///
/// Computer-use has a different rhythm to a structured tool: read, click, read
/// again because the screen moved, click again. "Open System Settings and turn
/// on dark mode" is four rounds before the first useful click, and it hit the
/// limit of 5 mid-navigation. Structured tools do not need this many and
/// keeping their budget tight is what stops a confused model burning turns, so
/// the budget is raised only for the runs that have earned it — the first
/// screen call switches to this.
///
/// 30, not agent-notch's 100: at ~400ms a read and ~170ms a click plus a model
/// round-trip each, 30 rounds is already the better part of a minute, and a
/// spoken command that has not landed by then is not going to.
const MAX_SCREEN_ROUNDS: usize = 30;

/// How many times one read tool may run in a single command. Rounds are already
/// capped, but a round can hold any number of calls, and a model that varies an
/// argument slips past the identical-call cache indefinitely.
const MAX_CALLS_PER_TOOL: usize = 3;

/// The same, for screen tools. A screen read must be repeatable after every
/// action that changes the display — capping it at 3 is what made the agent
/// click blind on its fourth step, since the alternative offered to it was
/// acting on a stale layout.
const MAX_SCREEN_CALLS_PER_TOOL: usize = 40;

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
    /// A long-running engine task (browser or Mac app) took a step. Live progress only; the model
    /// sees the whole run in the tool result.
    Progress { name: String, text: String },
}

const TOOL_SYSTEM_PROMPT: &str = "\
You are a concise assistant acting on a spoken command. You can call tools to \
act on the user's connected apps (email, calendar, GitHub, and whatever else \
the tool list offers). Only call a tool when the command asks for such an \
action or needs that data; otherwise just answer. \
To find files, photos, screenshots, or documents ON THIS MAC, use the local \
file search tool (ALOUD_FINDER_SEARCH) — not email, contacts, or calendar. \
\"Find my images with X\", \"find that screenshot\", \"find my tax pdf\" are \
local searches. \
To open, launch or switch to applications, always use ALOUD_OPEN_APP, passing \
every app the user named in one call. Never drive Spotlight, the Dock or the \
Finder on screen to open something — clicking a Spotlight result is not how you \
open an app here. \
Tools that take a list — opening apps, gathering files — do the whole batch in \
a single call. When the user asks for many things at once, put them all in one \
call rather than calling the tool repeatedly: each tool may only run a few \
times per command, so repeated calls will be cut off before the job is done. \
To copy or move files somewhere, use ALOUD_COLLECT_FILES with the paths from a \
search — it handles every file in one call. \
To save, write or note something down as a file, use ALOUD_WRITE_FILE with the \
whole text in one call. \
When fetching many emails to summarise them, ask for them in ONE call with a \
large max_results and set verbose to false and include_payload to false — full \
message bodies are enormous and only the first few would fit. Sender, subject, \
date and preview are enough to summarise from. \
When several accounts are connected and the user names one — \"my work email\", \
an address, a nickname — pass it as the `account` argument. If they do not name \
one, omit it rather than guessing. \
To tidy or organise a folder, call ALOUD_ORGANIZE_FOLDER exactly once. It sorts \
every file in the folder in that single call, so never call it repeatedly and \
never move files one at a time. You choose the categories: pick ones that fit \
what the user likely has, and give each the file extensions that belong in it. \
For anything to do on a website — open a site, search it, play a video at a \
time, read a value off a page, open a section of a dashboard — call \
ALOUD_BROWSER_TASK once with the user's whole request as the goal; it runs every \
step itself. \
When no tool in the list does what was asked, you can drive the app the user is \
looking at directly: ALOUD_SCREEN_READ lists its real on-screen controls with \
their positions, and the screen click/type/key tools act on them. This is the \
fallback, not the first choice — a named tool is faster and more reliable where \
one exists, so reach for the screen tools only when nothing else fits. Always \
read before you click, never invent coordinates, and read again after anything \
that changes what is on screen, because the positions move. Never send two \
clicks in a row without reading in between — the first click changes the \
screen, so the second would use coordinates that no longer point at anything. \
One click, then a fresh read, then the next click. The screen tools act on the \
app that is currently in front, so open or switch to the right app before \
reading it, and give it a moment by reading again if the app was just \
launched. \
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

/// Whether a failure on the first round should move to the next brain rather
/// than wait: everything [`backoff`] would retry, plus the errors waiting cannot
/// fix — a key with no billing (Cerebras answers 402), a revoked key, a model id
/// the provider no longer serves.
fn fails_over(error: &LlmError) -> bool {
    backoff(error).is_some()
        || matches!(
            error,
            LlmError::Failed {
                status: 401..=404,
                ..
            }
        )
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
async fn verify(brain: &Brain, command: &str, answer: &str) -> Option<String> {
    let messages = [
        Message::system(VERIFIER_PROMPT),
        Message::user(format!("COMMAND: {command}\n\nFINAL ANSWER: {answer}")),
    ];
    let model = brain.model;
    let turn = brain
        .client
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
    brains: &[Brain],
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
    // Whether the user has approved screen control for this run. See the gate
    // below: screen actions confirm once, not per click.
    let mut screen_approved = false;
    // Raised to MAX_SCREEN_ROUNDS the first time the run drives the screen.
    // A command that never touches it keeps the tight structured-tool budget.
    let mut max_rounds = MAX_ROUNDS;

    if brains.is_empty() {
        anyhow::bail!("no model is configured for commands");
    }
    // Which of `brains` this run is on; only ever moves forward.
    let mut brain = 0usize;

    let mut round = 0usize;
    while round < max_rounds {
        if cancelled() {
            anyhow::bail!("cancelled");
        }
        let is_last = round + 1 == max_rounds;
        // By index: a `&Brain` parameter makes the returned future's borrow a
        // closure-local lifetime the compiler will not infer.
        let chat = |index: usize| {
            let current = &brains[index];
            let model = current.model;
            current
                .client
                .chat_with_tools(
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
            let error = match chat(brain).await {
                Ok(turn) => break turn,
                Err(e) => e,
            };
            // Another brain beats waiting out this one's budget. Only before
            // anything has run: Gemini 3 rejects a history holding another
            // model's tool calls, which lack its thought signatures.
            if round == 0
                && brain + 1 < brains.len()
                && error.downcast_ref::<LlmError>().is_some_and(fails_over)
            {
                tracing::warn!(%error, from = brains[brain].model, to = brains[brain + 1].model, "failing over");
                brain += 1;
                continue;
            }
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
                if let Some(missing) = verify(&brains[brain], command, &answer).await {
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
                    // The `while` needs this by hand where the old `for` did
                    // it: without it a re-opened run never advances and spins
                    // on the same round forever.
                    round += 1;
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
            // A screen read is never the same twice: the whole reason to call it
            // again is that something moved. Serving it from cache would hand
            // the model the layout from before its own click and send the next
            // click to a stale coordinate.
            let repeatable = name == aloud_computer::AX_READ;
            if !is_write(&name, tools) && !repeatable {
                if let Some(previous) = seen.get(&(name.clone(), args.to_string())) {
                    tracing::debug!(tool = %name, "identical read already made; reusing");
                    messages.push(Message::tool_result(&call.id, previous.clone()));
                    continue;
                }
            }
            if !is_write(&name, tools) {
                // Independent of the dedup above: a model that varies one
                // argument each time never hits the cache and would otherwise
                // loop until the round limit.
                let cap = if repeatable {
                    MAX_SCREEN_CALLS_PER_TOOL
                } else {
                    MAX_CALLS_PER_TOOL
                };
                let calls = budget.entry(name.clone()).or_insert(0);
                *calls += 1;
                if *calls > cap {
                    tracing::warn!(tool = %name, "per-tool budget exhausted");
                    messages.push(Message::tool_result(
                        &call.id,
                        format!(
                            "{{\"error\":\"{name} has already been called \
                             {cap} times this run and will not run \
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
            // Screen control is gated once per run, not once per action: a
            // computer-use sequence is click, read, click, type, and confirming
            // each is unusable — the user would be answering prompts faster than
            // the agent works. So the *first* screen action asks, and approval
            // covers the rest of the run. Coarser than the per-action gate the
            // structured tools get, and deliberately so; see COMPUTER_USE.md's
            // open question, which this does not claim to settle.
            // The moment a run starts driving the screen it needs the longer
            // budget: read, click, read again, click again is four rounds
            // before anything useful lands, and five was not enough to open
            // System Settings and reach a control inside it.
            if is_screen_tool(&name) && max_rounds == MAX_ROUNDS {
                tracing::info!("screen control engaged; raising the round budget");
                max_rounds = MAX_SCREEN_ROUNDS;
            }

            let is_screen = is_screen_write(&name);
            // Engine tasks gate inside themselves: each risky click (buy, send, delete, sign in)
            // pauses for a yes/no below, so a gate on the whole goal would ask twice for one action.
            let gate_needed =
                if aloud_engine::is_engine_tool(&name) || aloud_native::low_risk(&name) {
                    false
                } else if is_screen {
                    !screen_approved
                } else {
                    is_write(&name, tools)
                };

            let mut args = args;
            let declined = if gate_needed {
                match confirm(name.clone(), args.clone())
                    .instrument(tracing::info_span!("confirm", tool = %name, toolkit))
                    .await
                {
                    Some(approved) => {
                        args = approved;
                        // Only once the user has actually said yes. Set before
                        // the await, a model that emits click+type in one round
                        // let the second call skip the gate while the first was
                        // still on screen — and opening a second confirmation
                        // dropped the first one's reply channel, which resolves
                        // as "declined" with no prompt ever shown.
                        screen_approved = true;
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
                let result = if aloud_engine::is_engine_tool(&name) {
                    run_engine(&name, &args, &mut on_event, &ask, &cancelled)
                        .instrument(span.clone())
                        .await
                } else {
                    match run_native(&name, &args).instrument(span.clone()).await {
                        Some(native) => native,
                        None => match run_computer(&name, &args).instrument(span.clone()).await {
                            Some(screen) => screen,
                            None => {
                                composio
                                    .execute(&name, &args)
                                    .instrument(span.clone())
                                    .await
                            }
                        },
                    }
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

            if !declined && !is_write(&name, tools) && !aloud_engine::is_engine_tool(&name) {
                seen.insert((name.clone(), args.to_string()), result.clone());
            }
            messages.push(Message::tool_result(&call.id, result));
        }
        round += 1;
    }

    // Naming the mode matters: "5 rounds" and "30 rounds" are different
    // failures, and the message is what the panel shows the user.
    anyhow::bail!(
        "the command needed more than {max_rounds} rounds and was stopped before it finished"
    )
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

/// Runs a browser or Mac task on the Voice Mac engine to completion, streaming each step as
/// [`LoopEvent::Progress`] and turning the engine's pause before a risky click into a Yes/No
/// question through the same `ask` path as [`ASK_USER`]. Escape stops the task, not just the loop.
async fn run_engine<A>(
    name: &str,
    args: &serde_json::Value,
    on_event: &mut impl FnMut(LoopEvent),
    ask: &impl Fn(String) -> A,
    cancelled: &impl Fn() -> bool,
) -> String
where
    A: Future<Output = Option<String>>,
{
    let mut task = match aloud_engine::start(name, args).await {
        Some(Ok(task)) => task,
        Some(Err(e)) => return serde_json::json!({ "error": e }).to_string(),
        None => return serde_json::json!({ "error": "not an engine tool" }).to_string(),
    };
    loop {
        if cancelled() {
            let _ = task.stop().await;
            return r#"{"error":"the user cancelled the task"}"#.to_string();
        }
        match task.next().await {
            None => {}
            Some(aloud_engine::Update::Step(text)) => on_event(LoopEvent::Progress {
                name: name.to_string(),
                text,
            }),
            Some(aloud_engine::Update::NeedsApproval(question)) => {
                on_event(LoopEvent::Asked {
                    question: question.clone(),
                    options: vec!["Yes".to_string(), "No".to_string()],
                });
                let yes = matches!(ask(question).await.as_deref(), Some("Yes"));
                if task.approve(yes).await.is_err() {
                    return r#"{"error":"the Voice Mac engine stopped while waiting for approval"}"#
                        .to_string();
                }
            }
            Some(aloud_engine::Update::Finished(result)) => return result.to_string(),
        }
    }
}

/// Runs a screen-control tool, `None` when the name is not one of those — the
/// same fall-through contract as [`run_native`], so the dispatch stays one
/// chain of "is it mine?" rather than a prefix match.
async fn run_computer(name: &str, args: &serde_json::Value) -> Option<String> {
    let name = name.to_string();
    let args = args.clone();
    // The helpers are subprocesses and a traversal takes ~800ms, so this must
    // not run on the async executor.
    let outcome = tokio::task::spawn_blocking(move || aloud_computer::execute(&name, &args))
        .await
        .unwrap_or_else(|e| Some(Err(aloud_computer::HelperError::Failed(e.to_string()))))?;
    Some(match outcome {
        Ok(value) => value.to_string(),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    })
}

/// Whether a tool drives the screen and so falls under the once-per-run gate
/// rather than the per-action one.
///
/// Keyed off the crate's own constants rather than a name prefix, so renaming a
/// tool cannot silently drop it out of the gate.
fn is_screen_write(name: &str) -> bool {
    matches!(
        name,
        aloud_computer::AX_CLICK | aloud_computer::AX_TYPE | aloud_computer::AX_KEY
    )
}

/// Whether a tool is part of screen control at all, reads included. The round
/// budget keys off this rather than [`is_screen_write`]: a run that has started
/// reading the screen is already committed to the read-act-read rhythm, and
/// waiting for its first *write* to raise the budget spends rounds it needs.
fn is_screen_tool(name: &str) -> bool {
    is_screen_write(name) || name == aloud_computer::AX_READ
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

    /// "Open System Settings and turn on dark mode" died at the round limit:
    /// open, read, click, click is already four of five, before the agent has
    /// reached the control it needs. Screen runs get a budget matched to the
    /// read-act-read rhythm; structured runs keep the tight one.
    #[test]
    fn driving_the_screen_earns_a_longer_round_budget() {
        // The budget switch as the loop performs it, so the test fails if the
        // rule changes rather than only if the constants do.
        let budget_after = |first_tool: &str| {
            let mut max = MAX_ROUNDS;
            if is_screen_tool(first_tool) && max == MAX_ROUNDS {
                max = MAX_SCREEN_ROUNDS;
            }
            max
        };
        assert_eq!(budget_after("GMAIL_SEND_EMAIL"), MAX_ROUNDS);
        assert_eq!(budget_after("ALOUD_ORGANIZE_FOLDER"), MAX_ROUNDS);
        // The read counts, not just the writes: a run already reading the
        // screen is committed to that rhythm, and waiting for its first click
        // to raise the budget spends the rounds it needs.
        assert_eq!(budget_after(aloud_computer::AX_READ), MAX_SCREEN_ROUNDS);
        assert_eq!(budget_after(aloud_computer::AX_CLICK), MAX_SCREEN_ROUNDS);
    }

    /// Compile-time, because these are relationships between constants: a
    /// runtime assert on two `const`s is just clippy noise, but the invariants
    /// still have to hold. "open, read, click, click" is four rounds before the
    /// first useful step, which is what exhausted the old limit mid-navigation;
    /// and a screen read must survive one call per round or the agent is forced
    /// to click on a layout it can no longer see.
    const _: () = {
        assert!(MAX_SCREEN_ROUNDS >= 20);
        assert!(MAX_SCREEN_ROUNDS > MAX_ROUNDS);
        assert!(MAX_SCREEN_CALLS_PER_TOOL >= MAX_SCREEN_ROUNDS);
        assert!(MAX_SCREEN_CALLS_PER_TOOL > MAX_CALLS_PER_TOOL);
    };

    /// The screen read is the one tool whose whole purpose is returning
    /// something different each time. Serving it from the dedup cache hands the
    /// model the layout from before its own click, and the next click goes to a
    /// coordinate that has moved.
    #[test]
    fn a_screen_read_is_never_answered_from_cache() {
        // Mirrors the loop's own rule: a repeatable tool skips the dedup and
        // takes the larger budget.
        let repeatable = |name: &str| name == aloud_computer::AX_READ;
        let cap_for = |name: &str| {
            if repeatable(name) {
                MAX_SCREEN_CALLS_PER_TOOL
            } else {
                MAX_CALLS_PER_TOOL
            }
        };

        assert!(repeatable(aloud_computer::AX_READ));
        assert!(!repeatable("ALOUD_FINDER_SEARCH"));
        assert_eq!(cap_for(aloud_computer::AX_READ), MAX_SCREEN_CALLS_PER_TOOL);
        assert_eq!(cap_for("ALOUD_FINDER_SEARCH"), MAX_CALLS_PER_TOOL);
    }

    /// Screen control is gated once per run rather than per action, so the
    /// three acting tools must be recognised — and the read must not be, or the
    /// gate would fire on a tool that changes nothing.
    #[test]
    fn every_screen_tool_that_acts_is_gated_and_the_read_is_not() {
        assert!(is_screen_write(aloud_computer::AX_CLICK));
        assert!(is_screen_write(aloud_computer::AX_TYPE));
        assert!(is_screen_write(aloud_computer::AX_KEY));
        assert!(!is_screen_write(aloud_computer::AX_READ));
        assert!(!is_screen_write("GMAIL_SEND_EMAIL"));
    }

    /// The gate's whole contract in one place: the first screen action asks,
    /// later ones do not, and a refusal re-arms it rather than leaving the run
    /// approved. Mirrors the `mem::replace` in the loop.
    #[test]
    fn screen_control_asks_once_then_stays_approved_unless_refused() {
        let mut approved = false;
        let mut gate = |refuse: bool| {
            let needed = !std::mem::replace(&mut approved, true);
            if needed && refuse {
                approved = false;
            }
            needed
        };

        assert!(gate(false), "the first screen action must ask");
        assert!(!gate(false), "a second must not ask again");
        assert!(!gate(false));

        // A run where the user says no: the next action asks again.
        let mut approved = false;
        let mut gate = |refuse: bool| {
            let needed = !std::mem::replace(&mut approved, true);
            if needed && refuse {
                approved = false;
            }
            needed
        };
        assert!(gate(true), "the first asks");
        assert!(
            gate(false),
            "after a refusal it must ask again, not proceed"
        );
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

    /// An unpaid Cerebras key answers 402 instantly and forever: waiting cannot
    /// fix it, the next brain can. A malformed request is the same everywhere.
    #[test]
    fn unpaid_and_throttled_brains_fail_over_but_bad_requests_do_not() {
        let failed = |status| LlmError::Failed {
            status,
            body: String::new(),
        };
        assert!(fails_over(&failed(402)));
        assert!(fails_over(&failed(503)));
        assert!(fails_over(&LlmError::RateLimited {
            model: "qwen-3.8-27b".into(),
            retry_after: Duration::from_secs(60),
        }));
        assert!(!fails_over(&failed(400)));
        assert!(!fails_over(&LlmError::EmptyOutput));
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
