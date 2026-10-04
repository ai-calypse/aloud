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
use crate::permissions::{Gate, RunTaint};
use crate::preview::Names;
use crate::Brain;
use aloud_llm::{LlmError, Message};
use anyhow::Result;
use futures_util::StreamExt;
use std::future::Future;
use std::time::Duration;
use tracing::Instrument;

/// How many of one round's calls run at once. "Send this, search that, note
/// the other" is three; a round asking for more is usually several reads
/// against one service's rate limit, where waiting costs little.
const MAX_PARALLEL: usize = 3;

/// The screen, keyboard focus and the clipboard: one tool drives them at a
/// time, across every run in the process. Held in [`dispatch`] by the engine,
/// screen control and native tools that take the front
/// (`aloud_native::takes_front`); a screen session holds it until the run ends
/// (see [`ScreenSession`]). Background AppleScript and Composio never wait on it.
static UI: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The longest one wait on a rate limit may hold a spoken command, whatever
/// the server or the breaker says; past it we would rather fail and say so.
const RETRY_CAP: Duration = Duration::from_secs(30);

/// Rounds allowed a command that only calls structured tools. Five covers the
/// shape those have — look something up, act on it, confirm — and a model still
/// going after five is looping rather than working.
const MAX_ROUNDS: usize = 5;

/// Rounds allowed once a command starts driving the screen. Screen control is
/// read, click, read the result, click again: "open System Settings and turn on
/// dark mode" is four rounds before the useful click, and hit the limit of 5
/// mid-navigation. Raised only for runs that touch the screen, so a confused
/// model cannot burn thirty turns on a command that should take three.
///
/// 30, not more: a driver action is ~1.5-2s plus a model round-trip, so thirty
/// rounds is already most of a minute.
const MAX_SCREEN_ROUNDS: usize = 30;

/// How many screen reads one command may make. A read must be repeatable after
/// every change, or the agent clicks on a layout it can no longer see.
const MAX_SCREEN_READS: usize = 40;

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

/// Loads a connected service's tools mid-run. The command's tools leave out
/// services it doesn't mention (see `composio::concerns`); this is how the model
/// gets one back when the command needs it after all — Anthropic's tool-search
/// pattern, a round trip on a miss instead of every schema on every command.
pub const MORE_TOOLS: &str = "ALOUD_MORE_TOOLS";

fn more_tools_schema(services: &[String]) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": MORE_TOOLS,
            "description": format!(
                "Load the tools for a connected service that is not in your list yet, when the \
                 command needs it: {}. Its tools are available on your next step.",
                services.join(", ")
            ),
            "parameters": {
                "type": "object",
                "required": ["service"],
                "properties": { "service": { "type": "string", "enum": services } }
            }
        }
    })
}

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
    ///
    /// `step` numbers the run's calls from 0. A round's calls all start before
    /// any finishes, so a result finds its own row by it, not by arriving last.
    ToolCallStart {
        step: u32,
        name: String,
        args: serde_json::Value,
    },
    /// That tool finished (success carries no payload; the model gets the JSON).
    /// `show` is what the row's Show button brings back, as (app, target); see
    /// `aloud_native::show_target`. Results come in call order, whichever
    /// finished first.
    ToolCallResult {
        step: u32,
        name: String,
        success: bool,
        show: Option<(String, String)>,
        /// The app a write that ran acted in, as its permission names it, for
        /// the audit log. None for a read, and for a write that never ran.
        write_app: Option<String>,
    },
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
You are Aloud, a voice assistant on the user's Mac. Each command is one spoken \
sentence, transcribed, so it may contain small errors; act on the intent. Either \
do it with tools or just answer. \
Choose tools in this order. \
First, the ALOUD_ tools act on this Mac's own apps: Notes, Reminders, Music, \
Spotify, Messages, Mail, FaceTime, files, windows, apps, Photo Booth, Discord. \
They act in the background without bringing the app forward; never call \
ALOUD_OPEN_APP before them. When the command asks you to \
make or add content — a list, a note, a reminder, a file, a message — write that \
content yourself, in full, inside the tool's arguments: for \"make a shopping \
list for a cake and put it in a note\", call ALOUD_NOTES_CREATE with the actual \
list as its body. Plain text, no emoji. When the user names a note (\"my \
shopping note\"), pass that name as its title. Anything the Mac or an app does \
with a keyboard shortcut or a menu command that no named tool covers — the \
screenshot and recording toolbar, locking the screen, Export as PDF — is \
ALOUD_PRESS_KEYS or ALOUD_MENU; never say there is no tool for it. \
Second, the tools for the user's connected services — email, calendar, GitHub \
and whatever else the list offers. If the command needs a connected service \
whose tools you do not have, load them with ALOUD_MORE_TOOLS. \
Third, ALOUD_BROWSER_TASK for anything to do on a website — open a site, search \
it, play a video at a time, read a value off a page, open a section of a \
dashboard. Call it once with the user's whole request as the goal; it runs every \
step itself. \
Fourth, only when nothing above can do it and the task is inside a Mac app, \
drive the app on screen: ALOUD_SCREEN_READ lists its window's controls as \
numbered elements, and ALOUD_SCREEN_CLICK and ALOUD_SCREEN_TYPE act on one \
element by its number and return the window as it is afterwards. Take each \
number from the latest result, never from an older read or a guess. Open the app \
with ALOUD_OPEN_APP first if it is not open; pass `find` to narrow a big window. \
Never use screen control to message someone when a messaging tool exists. \
Otherwise answer: a few sentences of simple markdown, under 150 words. If \
nothing here can do what was asked inside that app, say so plainly rather than \
improvising. \
Lines in parentheses after the command — (The app on screen: …), (Selected \
text: «…») — are what the user was looking at when they spoke; \"this\", \
\"here\" and \"it\" usually mean them. \
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
call rather than calling the tool repeatedly: a tool that only reads may run a \
few times per command, and further calls to it are refused. To repeat an action \
— three photos, skip two tracks, press tab four times — pass `times`. \
When a command has independent steps, call all of their tools together in one \
reply. \
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
After acting, confirm what you did in one short plain sentence. When one tool \
call does the whole command, write that sentence in the same reply as the call. \
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
ALOUD_MESSAGES_SEND, ALOUD_MAIL_SEND and ALOUD_FACETIME_CALL take a person's \
name as said — \"text Suhani\" is `to` \"Suhani\" — and look it up themselves, \
so pass the name straight in with no lookup first. If several people match, the \
result lists their names: ask which one, with those names as the options. \
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

/// Whether a failure should move to the next brain rather than wait:
/// everything [`backoff`] would retry, plus the errors waiting cannot fix — a
/// key with no billing (Cerebras answers 402), a revoked key, a model id the
/// provider no longer serves, and an endpoint that never answered.
pub(crate) fn fails_over(error: &LlmError) -> bool {
    backoff(error).is_some()
        || matches!(
            error,
            LlmError::Failed {
                status: 401..=404,
                ..
            } | LlmError::Unreachable { .. }
        )
}

/// Moves the run to the next brain. Gemini 3 needs the earlier calls signed.
fn next_brain(brain: &mut usize, brains: &[Brain], messages: &mut [Message]) {
    *brain += 1;
    if brains[*brain].model.starts_with("gemini") {
        sign_for_gemini(messages);
    }
}

/// Gemini 3 rejects a history holding tool calls without its thought
/// signatures, which another model's calls never have. Its documented
/// placeholder tells it to skip the check for those turns (LiteLLM does the
/// same on a model switch); the cost is the reasoning context they never had.
fn sign_for_gemini(messages: &mut [Message]) {
    for call in messages
        .iter_mut()
        .filter_map(|m| m.tool_calls.as_mut())
        .flatten()
        .filter(|c| c.extra_content.is_none())
    {
        call.extra_content = Some(serde_json::json!({
            "google": { "thought_signature": "skip_thought_signature_validator" }
        }));
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
    if let LlmError::RateLimited { retry_after, .. } = error {
        return (*retry_after).min(RETRY_CAP);
    }
    Duration::from_secs_f64(schedule[attempt] + jitter).min(RETRY_CAP)
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

/// The schemas and messages every run starts from.
fn opening(
    command: &str,
    tools: &[ToolDef],
    more: &[String],
) -> (Vec<serde_json::Value>, Vec<Message>) {
    // The synthetic tools ride alongside Composio's; the model cannot tell them
    // apart, and `execute` never sees them.
    let mut schemas: Vec<serde_json::Value> = tools.iter().map(ToolDef::to_openai).collect();
    schemas.push(ask_user_schema());
    if !more.is_empty() {
        schemas.push(more_tools_schema(more));
    }
    let messages = vec![
        Message::system(format!("{TOOL_SYSTEM_PROMPT} {}", now_line())),
        Message::user(command),
    ];
    (schemas, messages)
}

/// The brain's first move on `command` — what it would call, or answer — with
/// nothing executed. For the command eval: the choice made here is what
/// decides whether a command goes to the right place.
pub async fn first_turn(
    brain: &Brain,
    command: &str,
    tools: &[ToolDef],
    more: &[String],
) -> Result<aloud_llm::ToolTurn> {
    let (schemas, messages) = opening(command, tools, more);
    brain
        .client
        .chat_with_tools(brain.model, &messages, Some(&schemas), Some(0.3))
        .await
}

/// What the loop is asked to do: the user's words, the prompt built from them
/// and their context, and the tools on offer.
pub struct Command<'a> {
    /// The user's words only, never the context lines — what is checked for
    /// several actions or named apps, which a pasted selection must not sway.
    pub said: &'a str,
    /// `with_context(said, …)`: what the model is given.
    pub prompt: &'a str,
    pub tools: &'a [ToolDef],
    /// Connected services left out of `tools`, loadable via ALOUD_MORE_TOOLS.
    pub more: &'a [String],
    /// The app, its window's file and an editor's project when the user spoke —
    /// what a native tool with no target falls back on. See
    /// `aloud_native::with_front`.
    pub front: Option<&'a str>,
    pub document: Option<&'a str>,
    pub project: Option<&'a str>,
    /// Text was selected when the user spoke: outside content from the first
    /// round, for the named-app rule (see `permissions::RunTaint`).
    pub selection: bool,
}

/// A write the gate is asked about: the call as the model made it, and the
/// offered tool it names — from the run's own list, so a service loaded
/// mid-run is described by its own toolkit rather than a guess from the slug.
pub struct ProposedCall {
    pub name: String,
    pub args: serde_json::Value,
    pub tool: Option<ToolDef>,
    /// `Command::said`, for the card to tag the user's own words.
    pub said: String,
    /// What the run's reads have named so far, so the card shows "#launch"
    /// rather than the id the model copied out of a list.
    pub names: Names,
    /// The app and action its saved grant is keyed by, and whether the
    /// named-app rule fired. The caller holds the grants, so it decides.
    pub gate: Option<Gate>,
    /// Its `LoopEvent::ToolCallStart::step`, so the card lands on its own
    /// row; None for a call that never went through the loop.
    pub step: Option<u32>,
}

/// The gate's answer: run with these arguments (the user may have edited
/// them), or don't.
pub enum Verdict {
    Run(serde_json::Value),
    Decline,
}

/// Stands in for every tool when a run is evaluated: the canned result for a
/// name and its arguments, so a whole run can be scored with nothing executed.
pub type Mock<'a> = dyn Fn(&str, &serde_json::Value) -> String + Sync + 'a;

/// How the loop reaches the outside world.
///
/// `on_event` fires for each tool call so the panel can show progress.
/// `confirm` is awaited before any write runs, one call at a time and never
/// while a tool is running, so two cards are never up at once. `ask` waits on
/// the user's reply to a question. `cancelled` is polled at the top of each
/// round, between the calls a round admits, and before each queued call
/// starts, so Escape during a six-tool round stops the ones not yet running.
pub struct Callbacks<'a, E, C, A, X> {
    pub on_event: E,
    pub confirm: C,
    pub ask: A,
    pub cancelled: X,
    /// Eval only; see [`Mock`].
    pub mock: Option<&'a Mock<'a>>,
}

/// Runs the loop for `command`. Returns the model's final answer.
pub async fn run<E, C, F, A, Q, X>(
    brains: &[Brain],
    composio: &ComposioClient,
    command: Command<'_>,
    callbacks: Callbacks<'_, E, C, A, X>,
) -> Result<String>
where
    E: FnMut(LoopEvent),
    C: Fn(ProposedCall) -> F,
    F: Future<Output = Verdict>,
    A: Fn(String) -> Q,
    Q: Future<Output = Option<String>>,
    X: Fn() -> bool,
{
    let Callbacks {
        on_event,
        confirm,
        ask,
        cancelled,
        mock,
    } = callbacks;
    // Behind a lock so a round's calls, running together, can each report
    // progress. Never held across an await.
    let events = std::sync::Mutex::new(on_event);
    let emit = |event: LoopEvent| {
        let mut on_event = events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (*on_event)(event);
    };
    let Command {
        said,
        prompt: command,
        tools,
        more,
        front,
        document,
        project,
        selection,
    } = command;
    // A command with several steps can't end on its first round's writes:
    // "take 5 photos" ended after one.
    let several = names_several_actions(said);
    let (mut schemas, mut messages) = opening(command, tools, more);
    // Owned: a service loaded mid-run adds to it.
    let mut tools = tools.to_vec();

    // The empty-answer fallback must not say "Done." over a failure.
    let mut any_failed = false;
    // Reads already made this run, keyed by tool and exact arguments, and how
    // many times each tool has run. Both are per-run: a fresh command asks
    // fresh questions.
    let mut seen: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    let mut budget: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // Every id those reads returned, by name, for the gate's card.
    let mut names = Names::default();
    let mut asked = 0usize;
    let mut verifier_retries = 0;
    // Whether every tool call so far was a native write that worked; None
    // before the first. See `native_write`.
    let mut only_native_writes: Option<bool> = None;

    if brains.is_empty() {
        anyhow::bail!("no model is configured for commands");
    }
    // Which of `brains` this run is on; only ever moves forward.
    let mut brain = 0usize;
    // Raised the first time the run touches the screen; see MAX_SCREEN_ROUNDS.
    let mut max_rounds = MAX_ROUNDS;
    // Screen control asks once per run, not per click; see `screen_gate`.
    let mut screen_approved = false;
    let mut screen = ScreenSession {
        driving: false,
        ui: None,
    };
    // What this run's own screen-taking calls queue on once its screen
    // session holds `UI`, rather than waiting on themselves.
    let own_ui = tokio::sync::Mutex::const_new(());
    let mut taint = RunTaint::new(selection);
    let mut next_step = 0u32;

    let mut round = 0usize;
    while round < max_rounds {
        if cancelled() {
            anyhow::bail!("cancelled");
        }
        let is_last = round + 1 == max_rounds;
        // Per round, not per run: a run that survived a 429 in round one still
        // deserves a full ladder if round three trips a different limit.
        let mut attempts = 0usize;
        // Set by a backoff sleep, which already waited out the breaker's entry
        // for that 429; checking it again would wait twice.
        let mut waited = false;

        // Overload and transient 5xx are retried on a backoff ladder: the loop
        // makes several calls in a row, so a per-minute budget can trip between
        // rounds and a single bad upstream box can end a command that was
        // otherwise going fine. Here rather than in the llm crate so the panel
        // can show the wait instead of looking hung.
        let turn = loop {
            // Inline rather than a closure: the request borrows the history
            // only until it returns, so a mid-run failover can sign it for
            // Gemini between two calls.
            let current = &brains[brain];
            let model = current.model;
            // The process-wide breaker may already know this model is out of
            // budget, from an earlier round or another command. The next brain
            // beats waiting; on the last one this is the wait a 429 would have
            // cost, without spending the request.
            let cooling = if std::mem::take(&mut waited) {
                None
            } else {
                current.client.cooldown_remaining(model)
            };
            if let Some(wait) = cooling {
                if brain + 1 < brains.len() {
                    tracing::info!(
                        from = model,
                        to = brains[brain + 1].model,
                        wait_s = wait.as_secs_f64(),
                        round,
                        "cooling down; failing over"
                    );
                    next_brain(&mut brain, brains, &mut messages);
                    continue;
                }
                let wait = wait.min(RETRY_CAP);
                emit(LoopEvent::RateLimited {
                    seconds: wait.as_secs().max(1),
                });
                async { tokio::time::sleep(wait).await }
                    .instrument(tracing::info_span!(
                        "tool_select_cooldown",
                        round,
                        waited_s = wait.as_secs_f64()
                    ))
                    .await;
                if cancelled() {
                    anyhow::bail!("cancelled");
                }
            }
            let reply = current
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
                .await;
            let error = match reply {
                Ok(turn) => break turn,
                Err(e) => e,
            };
            // Another brain beats waiting out this one's budget — mid-run too:
            // Groq's free tier trips on a command's second step, and NVIDIA's
            // free endpoint sometimes never answers.
            if brain + 1 < brains.len() && error.downcast_ref::<LlmError>().is_some_and(fails_over)
            {
                tracing::warn!(%error, from = brains[brain].model, to = brains[brain + 1].model, round, "failing over");
                next_brain(&mut brain, brains, &mut messages);
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
            emit(LoopEvent::RateLimited {
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
            waited = true;
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
            // Not for a run of native writes that all worked: the grader sees
            // only the command and the answer, and its real target is a
            // connected service drafting without sending.
            if verifier_retries < MAX_VERIFIER_RETRIES
                && !is_last
                && !cancelled()
                && only_native_writes != Some(true)
            {
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
            emit(LoopEvent::Planned {
                names: turn
                    .tool_calls
                    .iter()
                    .map(|c| c.function.name.clone())
                    .collect(),
            });
        }

        // Cleared by any call that is not a native write that worked.
        let mut round_native_ok = true;
        // What each of those writes did, in its own words; see `finishes_early`.
        let mut confirmations: Vec<Option<String>> = Vec::new();
        // Every call is admitted in order first — budgets, questions, the gates
        // one card at a time — and only then do the approved ones run,
        // together. Each was chosen and approved before any result came back,
        // so no page or message one of them reads can steer a sibling; later
        // rounds are the named-app rule's.
        let mut slots: Vec<Slot> = Vec::with_capacity(turn.tool_calls.len());
        for call in &turn.tool_calls {
            // Between admissions, not just at the top of the round: Escape
            // during a gate must not leave the rest of the round to run.
            if cancelled() {
                anyhow::bail!("cancelled");
            }

            let name = call.function.name.clone();
            let args: serde_json::Value = serde_json::from_str(&call.function.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));
            round_native_ok &= native_write(&name, &args, &tools);
            let done = |result: String| Slot::Done {
                id: call.id.clone(),
                result,
            };

            if name == MORE_TOOLS {
                let service = args["service"].as_str().unwrap_or_default().to_string();
                let result = if !more.contains(&service) {
                    format!("{{\"error\":\"{service} is not a connected service\"}}")
                } else if tools.iter().any(|t| t.toolkit == service) {
                    format!("{{\"loaded\":\"{service}\",\"note\":\"already loaded\"}}")
                } else {
                    match composio
                        .toolkit_tools(&service, command, crate::composio::TOOLS_PER_TOOLKIT)
                        .await
                    {
                        Ok(found) => {
                            schemas.extend(found.iter().map(ToolDef::to_openai));
                            let names: Vec<String> = found.iter().map(|t| t.name.clone()).collect();
                            tools.extend(found);
                            serde_json::json!({ "loaded": service, "tools": names }).to_string()
                        }
                        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
                    }
                };
                slots.push(done(result));
                continue;
            }

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
                    slots.push(done(
                        r#"{"error":"no more questions can be asked this run. Act on what you have and state any assumption in your answer."}"#.to_string(),
                    ));
                    continue;
                }
                emit(LoopEvent::Asked {
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
                // No result event: the question is the panel's whole UI for
                // it, and there is no row for a result to land on.
                slots.push(done(result));
                continue;
            }

            // A read that has already run this turn is answered from the first
            // result instead of being repeated. Looking one person up cost
            // three round-trips — the same inbox query twice, then a third — and
            // the model has no memory of having asked. Writes are excluded: two
            // identical sends are two emails, and collapsing them would swallow
            // the second silently.
            // Except a screen read: the reason to read again is that something
            // moved, and a cached one is the layout from before the click.
            if !is_write(&name, &tools) && name != aloud_screen::READ {
                if let Some(previous) = seen.get(&(name.clone(), args.to_string())) {
                    tracing::debug!(tool = %name, "identical read already made; reusing");
                    slots.push(done(previous.clone()));
                    continue;
                }
                // The same read twice in one round runs once.
                let twin = slots.iter().position(
                    |s| matches!(s, Slot::Run(job) if job.name == name && job.args == args),
                );
                if let Some(of) = twin {
                    slots.push(Slot::Same {
                        id: call.id.clone(),
                        of,
                    });
                    continue;
                }
            }
            if !is_write(&name, &tools) {
                // Independent of the dedup above: a model that varies one
                // argument each time never hits the cache and would otherwise
                // loop until the round limit.
                let cap = if name == aloud_screen::READ {
                    MAX_SCREEN_READS
                } else {
                    MAX_CALLS_PER_TOOL
                };
                let calls = budget.entry(name.clone()).or_insert(0);
                *calls += 1;
                if *calls > cap {
                    tracing::warn!(tool = %name, "per-tool budget exhausted");
                    slots.push(done(format!(
                        "{{\"error\":\"{name} has already been called \
                         {cap} times this run and will not run \
                         again. Answer from what you have.\"}}"
                    )));
                    continue;
                }
            }

            // Before the panel and the gate, so both show the person a spoken
            // name resolved to. Several or none goes back for the model to ask.
            let args = match prepare(&name, &args, mock).await {
                None => args,
                Some(Ok(resolved)) => resolved,
                Some(Err(error)) => {
                    any_failed = true;
                    round_native_ok = false;
                    slots.push(done(error.to_string()));
                    continue;
                }
            };

            let step = next_step;
            next_step += 1;
            emit(LoopEvent::ToolCallStart {
                step,
                name: name.clone(),
                args: args.clone(),
            });

            // The toolkit drives the latency profile — a Gmail send and a
            // Calendar create behave nothing alike — so it is a span field
            // rather than something to infer from the tool name.
            let toolkit = tools
                .iter()
                .find(|t| t.name == name)
                .map_or_else(|| "unknown".to_string(), |t| t.toolkit.clone());

            // Confirmation is the user thinking, not the app working: its own
            // span so a 40s wait is not charged to pipeline latency. A declined
            // write returns as a normal result so the model can say "cancelled"
            // rather than erroring the whole command.
            // Rebound, not just checked: the gate returns the arguments to run
            // with, so a field the user filled in at the gate is what executes.
            // A read tool never reaches the gate and keeps the model's.
            // Engine tasks gate inside themselves: each risky click (buy, send, delete, sign in)
            // pauses for a yes/no below, so a gate on the whole goal would ask twice for one action.
            if aloud_screen::is_screen_tool(&name) && max_rounds == MAX_ROUNDS {
                tracing::info!("screen control engaged; raising the round budget");
                max_rounds = MAX_SCREEN_ROUNDS;
                // A mocked run drove no screen, so there is no session to end.
                screen.driving = mock.is_none();
            }
            // Every other write reaches `confirm`, default-Always ones included:
            // the saved grants live with the caller, which decides whether a
            // card is shown. Screen control still asks once per run.
            let tool = tools.iter().find(|t| t.name == name).cloned();
            let gate = crate::permissions::gate(&name, &args, tool.as_ref(), said, front, &taint);
            let gate_needed = gate.is_some()
                && (!aloud_screen::is_screen_write(&name) || screen_gate(&mut screen_approved));
            let write_app = gate.as_ref().map(|g| g.app.clone());

            let mut args = args;
            let declined = if gate_needed {
                // A yes to what outside content asked for names that app for
                // the rest of the run.
                let named = gate
                    .as_ref()
                    .filter(|g| g.named_rule)
                    .map(|g| g.app.clone());
                let proposed = ProposedCall {
                    name: name.clone(),
                    args: args.clone(),
                    tool,
                    said: said.to_string(),
                    names: names.clone(),
                    gate,
                    step: Some(step),
                };
                match confirm(proposed)
                    .instrument(tracing::info_span!("confirm", tool = %name, %toolkit))
                    .await
                {
                    Verdict::Run(approved) => {
                        args = approved;
                        if let Some(app) = named {
                            taint.clear(&app);
                        }
                        false
                    }
                    Verdict::Decline => {
                        // A no re-arms the screen gate rather than leaving the
                        // run approved for the next click.
                        if aloud_screen::is_screen_write(&name) {
                            screen_approved = false;
                        }
                        true
                    }
                }
            } else {
                false
            };

            if declined {
                round_native_ok = false;
                slots.push(Slot::Declined {
                    id: call.id.clone(),
                    step,
                    name,
                });
                continue;
            }
            slots.push(Slot::Run(Job {
                id: call.id.clone(),
                step,
                name,
                args,
                toolkit,
                write_app,
            }));
        }

        // A screen session keeps the screen until the run ends, so no other
        // run brings its app forward between two clicks. Taken before the
        // first screen call runs; this run's own calls then queue on `own_ui`.
        let drives_screen = slots
            .iter()
            .any(|s| matches!(s, Slot::Run(job) if aloud_screen::is_screen_tool(&job.name)));
        if drives_screen && screen.ui.is_none() {
            screen.ui = Some(UI.lock().await);
        }
        let ui = if screen.ui.is_some() { &own_ui } else { &UI };
        // Collected before the stream, not mapped inside it: a stream over a
        // closure taking `&Slot` trips rustc's higher-ranked Send check
        // (rust-lang/rust#102211) where the app spawns the run.
        let jobs: Vec<_> = slots
            .iter()
            .map(|slot| {
                let (emit, ask, cancelled) = (&emit, &ask, &cancelled);
                async move {
                    let Slot::Run(job) = slot else {
                        return None;
                    };
                    // A call still queued when the user pressed Escape never starts.
                    if cancelled() {
                        return None;
                    }
                    let span = tracing::info_span!(
                        "tool_exec",
                        tool = %job.name,
                        toolkit = %job.toolkit,
                        ok = tracing::field::Empty
                    );
                    // A native tool falls back on where the user was when they
                    // spoke. Merged here — after the start event, the gate and the
                    // dedup key — so neither the model, the panel nor the
                    // confirmation sees it.
                    let exec_args = if job.toolkit == "macOS" {
                        aloud_native::with_front(&job.args, front, document, project)
                    } else {
                        job.args.clone()
                    };
                    let mut progress = |event| emit(event);
                    let result = dispatch(
                        &job.name,
                        &exec_args,
                        composio,
                        mock,
                        &mut progress,
                        ask,
                        cancelled,
                        ui,
                    )
                    .instrument(span.clone())
                    .await;
                    span.record("ok", tool_succeeded(&result));
                    Some(result)
                }
            })
            .collect();
        let outcomes: Vec<Option<String>> = futures_util::stream::iter(jobs)
            .buffered(MAX_PARALLEL)
            .collect()
            .await;
        if cancelled() {
            anyhow::bail!("cancelled");
        }

        // To the model and the panel in call order, whichever finished first.
        let mut results: Vec<String> = Vec::with_capacity(slots.len());
        for (slot, outcome) in slots.into_iter().zip(outcomes) {
            let (id, result) = match slot {
                Slot::Done { id, result } => (id, result),
                Slot::Same { id, of } => (id, results[of].clone()),
                Slot::Declined { id, step, name } => {
                    emit(LoopEvent::ToolCallResult {
                        step,
                        name,
                        success: false,
                        show: None,
                        write_app: None,
                    });
                    (id, r#"{"error":"user declined this action"}"#.to_string())
                }
                Slot::Run(job) => {
                    // Only a cancel leaves it empty, and that bailed above.
                    let result = outcome.unwrap_or_default();
                    let success = tool_succeeded(&result);
                    let read = !is_write(&job.name, &tools);
                    let engine = aloud_engine::is_engine_tool(&job.name);
                    // Text someone else wrote is in the context from here on.
                    taint.saw(read || engine || aloud_screen::is_screen_tool(&job.name));
                    any_failed |= !success;
                    round_native_ok &= success;
                    let show = if success {
                        let parsed = serde_json::from_str(&result).unwrap_or_default();
                        confirmations
                            .push(aloud_native::confirmation(&job.name, &job.args, &parsed));
                        aloud_native::show_target(&job.name, &job.args, &parsed)
                    } else {
                        None
                    };
                    emit(LoopEvent::ToolCallResult {
                        step: job.step,
                        name: job.name.clone(),
                        success,
                        show,
                        write_app: job.write_app,
                    });
                    if read && !engine && job.name != aloud_screen::READ {
                        if let Ok(parsed) = serde_json::from_str(&result) {
                            names.harvest(&parsed, &query_of(&job.name, &job.args));
                        }
                        seen.insert((job.name, job.args.to_string()), result.clone());
                    }
                    (job.id, result)
                }
            };
            messages.push(Message::tool_result(&id, result.clone()));
            results.push(result);
        }
        only_native_writes = Some(only_native_writes.unwrap_or(true) && round_native_ok);
        // The model already said what it did, and what it did was ours and
        // worked: asking it again to say so, then grading that, was two more
        // model calls (~0.8s) on every "create a note". Not for a command that
        // names several steps, where the first round is rarely all of them.
        if let Some(said) = finishes_early(turn.content.as_deref(), round_native_ok, &confirmations)
            .filter(|_| !several)
        {
            return Ok(said);
        }
        round += 1;
    }

    // Naming the budget matters: 5 rounds and 30 are different failures, and
    // the message is what the panel shows.
    anyhow::bail!(
        "the command needed more than {max_rounds} rounds and was stopped before it finished"
    )
}

/// Whether this screen action must ask first, marking the run approved. Once
/// per run, not per click: a screen task is read, click, read, type, and a
/// prompt per step has the user answering faster than the agent works. Marked
/// up front so a model that sends click and type in one round cannot slip the
/// second past while the first is on screen; a no re-arms it.
fn screen_gate(approved: &mut bool) -> bool {
    !std::mem::replace(approved, true)
}

/// A run's hold on the screen once it drives it. However the run ends —
/// answer, error or cancel — the driver session ends first, so the next run
/// starts with no stale snapshots, and only then is `ui` released for other
/// runs' screen-taking tools (fields drop after `drop` returns).
struct ScreenSession {
    /// A real driver session to end; a mocked run drove none.
    driving: bool,
    ui: Option<tokio::sync::MutexGuard<'static, ()>>,
}

impl Drop for ScreenSession {
    fn drop(&mut self) {
        if self.driving {
            aloud_screen::finish();
        }
    }
}

/// One call of a round once it has been admitted, waiting to be reported.
enum Slot {
    /// Answered without running a tool: a question, a loaded service, a read
    /// already made, a spent budget, a lookup that found nobody.
    Done {
        id: String,
        result: String,
    },
    /// The same read as an earlier slot this round, answered from it.
    Same {
        id: String,
        of: usize,
    },
    Declined {
        id: String,
        step: u32,
        name: String,
    },
    Run(Job),
}

/// An approved call, with the arguments it runs with.
struct Job {
    /// The model's id for the call, which its result answers.
    id: String,
    step: u32,
    name: String,
    args: serde_json::Value,
    toolkit: String,
    write_app: Option<String>,
}

/// Runs a screen-control tool; None when the name is not one, the same
/// fall-through contract as [`run_native`].
async fn run_screen(name: &str, args: &serde_json::Value) -> Option<String> {
    if !aloud_screen::is_screen_tool(name) {
        return None;
    }
    let (name, args) = (name.to_string(), args.clone());
    // Each step is a driver subprocess and an action takes seconds.
    let outcome = tokio::task::spawn_blocking(move || aloud_screen::execute(&name, &args))
        .await
        .unwrap_or_else(|e| Some(serde_json::json!({ "error": e.to_string() })))?;
    Some(outcome.to_string())
}

/// Runs one tool. Native macOS tools are ours; a Composio slug is not, so each
/// "is it mine?" returns None and the call falls through to the next. One
/// chain, because the gate, dedup and budgets above all key off the tool name.
///
/// A tool that drives the screen holds `ui` while it runs: [`UI`], or the
/// run's own lock once its screen session holds that.
#[allow(clippy::too_many_arguments)]
async fn dispatch<A>(
    name: &str,
    args: &serde_json::Value,
    composio: &ComposioClient,
    mock: Option<&Mock<'_>>,
    on_event: &mut impl FnMut(LoopEvent),
    ask: &impl Fn(String) -> A,
    cancelled: &impl Fn() -> bool,
    ui: &tokio::sync::Mutex<()>,
) -> String
where
    A: Future<Output = Option<String>>,
{
    // Before the mock, so an evaluated run queues exactly as a real one would.
    let _front = if takes_ui(name, args) {
        Some(ui.lock().await)
    } else {
        None
    };
    if let Some(mock) = mock {
        return mock(name, args);
    }
    if aloud_engine::is_engine_tool(name) {
        return run_engine(name, args, on_event, ask, cancelled).await;
    }
    if let Some(native) = run_native(name, args).await {
        return native;
    }
    if let Some(screen) = run_screen(name, args).await {
        return screen;
    }
    composio.execute(name, args).await
}

/// Whether a tool drives what is on screen, so it runs alone: the browser
/// engine, screen control, and native tools that take the front.
fn takes_ui(name: &str, args: &serde_json::Value) -> bool {
    aloud_engine::is_engine_tool(name)
        || aloud_screen::is_screen_tool(name)
        || aloud_native::takes_front(name, args)
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

/// [`aloud_native::prepare`] on a blocking thread, since a Contacts lookup is an
/// osascript run. Never under a mock: an eval must not read the real Contacts,
/// and its canned results stand in for whatever the lookup would have found.
async fn prepare(
    name: &str,
    args: &serde_json::Value,
    mock: Option<&Mock<'_>>,
) -> Option<Result<serde_json::Value, serde_json::Value>> {
    if mock.is_some() {
        return None;
    }
    let (name, args) = (name.to_string(), args.clone());
    tokio::task::spawn_blocking(move || aloud_native::prepare(&name, &args))
        .await
        .unwrap_or_else(|e| Some(Err(serde_json::json!({ "error": e.to_string() }))))
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

/// How a read found what it returned, in words a card can show under "picked
/// by": its text arguments ("category:promotions older_than:30d"), else the
/// tool itself.
fn query_of(name: &str, args: &serde_json::Value) -> String {
    let words: Vec<&str> = args
        .as_object()
        .into_iter()
        .flat_map(|a| a.values())
        .filter_map(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .collect();
    if words.is_empty() {
        name.to_string()
    } else {
        words.join(" ")
    }
}

/// Whether a call is one the run may finish on without asking the model again:
/// one of our own Mac actions that changes something and reports nothing the
/// model has yet to read. A search or "what's playing" returns what the answer
/// is about, so text written before it ran cannot be the answer.
fn native_write(name: &str, args: &serde_json::Value, tools: &[ToolDef]) -> bool {
    name.starts_with("ALOUD_")
        && name != ASK_USER
        && name != MORE_TOOLS
        && !aloud_engine::is_engine_tool(name)
        // A screen action is a step, never the end: the run reads what it did.
        && !aloud_screen::is_screen_tool(name)
        && is_write(name, tools)
        && args["action"] != "now_playing"
}

/// Whether the user's words ask for more than one step: "and", "then",
/// "twice", "3 photos", "4 times". A false hit only costs the round the early
/// finish would have saved; a miss ends a run with work left undone.
fn names_several_actions(said: &str) -> bool {
    // Not a bare comma: transcripts are full of them ("Okay, create a note").
    const LINKS: &[&str] = &[" and ", " then ", " also ", "after that"];
    const COUNTS: &[&str] = &["times", "twice", "thrice"];
    const NUMBERS: &[&str] = &[
        "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "fifteen", "twenty",
    ];
    // Plurals that measure time, not repetitions: "in 5 minutes", "for 2 hours".
    const UNITS: &[&str] = &[
        "seconds", "secs", "minutes", "mins", "hours", "hrs", "days", "weeks", "months", "years",
        "percent",
    ];
    let said = format!(" {} ", said.to_lowercase());
    if LINKS.iter().any(|l| said.contains(l)) {
        return true;
    }
    let words: Vec<String> = said
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    words.iter().any(|w| COUNTS.contains(&w.as_str()))
        || words.windows(2).any(|pair| {
            let is_count = pair[0].chars().all(|c| c.is_ascii_digit())
                && pair[0].parse::<u32>().is_ok_and(|n| n > 1)
                || NUMBERS.contains(&pair[0].as_str());
            is_count
                && pair[1].len() > 2
                && pair[1].ends_with('s')
                && !UNITS.contains(&pair[1].as_str())
        })
}

/// The run's answer, if this round can be the last: every call was a native
/// write that worked, and either the model already said what it did or each
/// write says so itself (`aloud_native::confirmation`). Most models put no text
/// beside a tool call — qwen did in 0 of 26 — so the second is the usual way.
fn finishes_early(
    content: Option<&str>,
    round_native_ok: bool,
    confirmations: &[Option<String>],
) -> Option<String> {
    if !round_native_ok {
        return None;
    }
    if let Some(said) = content.map(str::trim).filter(|c| !c.is_empty()) {
        return Some(said.to_string());
    }
    let own: Option<Vec<String>> = confirmations.iter().cloned().collect();
    own.filter(|o| !o.is_empty()).map(|o| o.join(" "))
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
            app: crate::composio::toolkit_of_slug(name),
            description: String::new(),
            parameters: serde_json::json!({}),
            read_only,
            hints: Vec::new(),
        }
    }

    /// A chat endpoint on localhost answering request `n` with `replies[n]`
    /// (the last repeats), and keeping every request body the loop sent it.
    /// A [`rate_limited`] reply is sent as a 429.
    fn fake_brain(
        replies: Vec<serde_json::Value>,
    ) -> (Brain, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let bodies = sent.clone();
        std::thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                let mut stream = stream.unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    match line.split_once(':') {
                        Some((key, value)) if key.eq_ignore_ascii_case("content-length") => {
                            length = value.trim().parse().unwrap();
                        }
                        _ if line.trim().is_empty() => break,
                        _ => {}
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                bodies
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
                let reply = &replies[n.min(replies.len() - 1)];
                let status = match reply["retry_after"].as_str() {
                    Some(seconds) => format!("429 Too Many Requests\r\nRetry-After: {seconds}"),
                    None => "200 OK".to_string(),
                };
                let reply = reply.to_string();
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
            }
        });
        let brain = Brain {
            client: aloud_llm::LlmClient::new("test", Some(&url)).unwrap(),
            model: "fake",
        };
        (brain, sent)
    }

    /// The front app reaches our own tools only, and only once the gate has
    /// passed: a connected service never gets it, and neither the model, the
    /// panel's start event nor the confirmation ever sees it.
    #[tokio::test]
    async fn the_front_app_reaches_native_tools_only_and_nothing_else_sees_it() {
        let menu = serde_json::json!({ "item": "Quit Safari" });
        let issue = serde_json::json!({ "title": "Bug" });
        let calls = serde_json::json!({ "choices": [{ "message": { "tool_calls": [
            { "id": "a", "type": "function",
              "function": { "name": "ALOUD_MENU", "arguments": menu.to_string() } },
            { "id": "b", "type": "function",
              "function": { "name": "GITHUB_CREATE_ISSUE", "arguments": issue.to_string() } },
        ] } }] });
        // The answer, then the grader's verdict on it.
        let finished = serde_json::json!({ "choices": [{ "message": { "content": "FINISHED" } }] });
        let (brain, requests) = fake_brain(vec![calls, finished]);

        let mut menu_tool = tool("ALOUD_MENU", false);
        menu_tool.toolkit = "macOS".into();
        let tools = [menu_tool, tool("GITHUB_CREATE_ISSUE", false)];
        let ran = std::sync::Mutex::new(Vec::new());
        let mock = |name: &str, args: &serde_json::Value| {
            ran.lock().unwrap().push((name.to_string(), args.clone()));
            r#"{"successful":true}"#.to_string()
        };
        let gated = std::sync::Mutex::new(Vec::new());
        let mut started = Vec::new();

        run(
            &[brain],
            &ComposioClient::new("").unwrap(),
            Command {
                said: "quit safari and file a bug",
                prompt: "quit safari and file a bug",
                tools: &tools,
                more: &[],
                front: Some("Safari"),
                document: Some("/Users/me/page.html"),
                project: None,
                selection: false,
            },
            Callbacks {
                on_event: |event| {
                    if let LoopEvent::ToolCallStart { args, .. } = event {
                        started.push(args);
                    }
                },
                confirm: |call: ProposedCall| {
                    gated.lock().unwrap().push(call.args.clone());
                    async move { Verdict::Run(call.args) }
                },
                ask: |_: String| async { None },
                cancelled: || false,
                mock: Some(&mock),
            },
        )
        .await
        .unwrap();

        let ran = ran.into_inner().unwrap();
        assert_eq!(ran[0].0, "ALOUD_MENU");
        assert_eq!(ran[0].1["item"], "Quit Safari");
        assert_eq!(ran[0].1["_front_app"], "Safari");
        assert_eq!(ran[0].1["_front_document"], "/Users/me/page.html");
        assert!(
            ran[0].1.get("_front_project").is_none(),
            "only what is known"
        );
        assert_eq!(ran[1], ("GITHUB_CREATE_ISSUE".to_string(), issue.clone()));
        assert_eq!(started, vec![menu.clone(), issue.clone()]);
        assert_eq!(gated.into_inner().unwrap(), vec![menu, issue]);
        let requests = requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            3,
            "a round of calls, the answer, the grader"
        );
        assert!(requests.iter().all(|body| !body.contains("_front_")));
    }

    /// The model lists channels, then posts to one by the id it copied out:
    /// the gate has to say "launch", which only the earlier read knew.
    #[tokio::test]
    async fn a_channel_id_from_an_earlier_read_in_the_run_shows_its_name_at_the_gate() {
        let call = |id: &str, name: &str, args: serde_json::Value| {
            serde_json::json!({ "choices": [{ "message": { "tool_calls": [
                { "id": id, "type": "function",
                  "function": { "name": name, "arguments": args.to_string() } },
            ] } }] })
        };
        let list = call(
            "a",
            "SLACK_FIND_CHANNELS",
            serde_json::json!({ "query": "launch" }),
        );
        let post = call(
            "b",
            "SLACK_CHAT_POST_MESSAGE",
            serde_json::json!({ "channel": "C0123ABCD45", "markdown_text": "hi" }),
        );
        let finished = serde_json::json!({ "choices": [{ "message": { "content": "FINISHED" } }] });
        let (brain, _) = fake_brain(vec![list, post, finished]);
        let tools = [
            tool("SLACK_FIND_CHANNELS", true),
            tool("SLACK_CHAT_POST_MESSAGE", false),
        ];
        let mock = |name: &str, _: &serde_json::Value| match name {
            "SLACK_FIND_CHANNELS" => {
                serde_json::json!({ "successful": true, "data": { "channels": [
                { "id": "C0123ABCD45", "name": "launch" },
                { "id": "C0999ZZZZ99", "name": "random" },
            ] } })
                .to_string()
            }
            _ => r#"{"successful":true}"#.to_string(),
        };
        let gates = std::sync::Mutex::new(Vec::new());

        run(
            &[brain],
            &ComposioClient::new("").unwrap(),
            Command {
                said: "send hi to the launch channel",
                prompt: "send hi to the launch channel",
                tools: &tools,
                more: &[],
                front: None,
                document: None,
                project: None,
                selection: false,
            },
            Callbacks {
                on_event: |_| {},
                confirm: |call: ProposedCall| {
                    let ctx = crate::preview::Context {
                        said: &call.said,
                        names: &call.names,
                    };
                    let card = crate::preview::render(&call.name, &call.args, None, &ctx);
                    gates.lock().unwrap().push(card);
                    async move { Verdict::Run(call.args) }
                },
                ask: |_: String| async { None },
                cancelled: || false,
                mock: Some(&mock),
            },
        )
        .await
        .unwrap();

        let gates = gates.into_inner().unwrap();
        let [card] = gates.as_slice() else {
            panic!("one gate, got {}", gates.len());
        };
        assert_eq!(card.title, "Send message to launch");
        let channel = &card.fields[0];
        assert_eq!(
            (channel.name.as_deref(), channel.unresolved),
            (Some("launch"), false)
        );
        assert_eq!(card.fields[1].origin, Some(crate::preview::Origin::Yours));
    }

    fn calls(names: &[&str]) -> serde_json::Value {
        let calls: Vec<serde_json::Value> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                serde_json::json!({ "id": i.to_string(), "type": "function",
                    "function": { "name": name, "arguments": "{}" } })
            })
            .collect();
        serde_json::json!({ "choices": [{ "message": { "tool_calls": calls } }] })
    }

    fn content(text: &str) -> serde_json::Value {
        serde_json::json!({ "choices": [{ "message": { "content": text } }] })
    }

    /// A 429 telling the client to come back in `seconds`; see [`fake_brain`].
    fn rate_limited(seconds: u32) -> serde_json::Value {
        serde_json::json!({ "retry_after": seconds.to_string() })
    }

    fn command<'a>(said: &'a str, tools: &'a [ToolDef]) -> Command<'a> {
        Command {
            said,
            prompt: said,
            tools,
            more: &[],
            front: None,
            document: None,
            project: None,
            selection: false,
        }
    }

    /// Plays `said` on `brains` with every tool answered by `mock` and every
    /// gate approved.
    async fn play(
        brains: &[Brain],
        said: &str,
        tools: &[ToolDef],
        mock: &Mock<'_>,
        on_event: impl FnMut(LoopEvent),
    ) -> Result<String> {
        run(
            brains,
            &ComposioClient::new("").unwrap(),
            command(said, tools),
            Callbacks {
                on_event,
                confirm: |call: ProposedCall| async move { Verdict::Run(call.args) },
                ask: |_: String| async { None },
                cancelled: || false,
                mock: Some(mock),
            },
        )
        .await
    }

    /// Holds the screen as another run would, letting go once `ran` holds
    /// `count` calls — which can only happen if those calls did not queue
    /// behind the one waiting for the screen.
    async fn hold_the_screen_until(ran: &std::sync::Mutex<Vec<String>>, count: usize) {
        let held = UI.lock().await;
        while ran.lock().unwrap().len() < count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop(held);
    }

    /// A mock that logs each call's name and answers with `{"successful":true}`.
    fn logging(
        ran: &std::sync::Mutex<Vec<String>>,
    ) -> impl Fn(&str, &serde_json::Value) -> String + Sync + '_ {
        move |name: &str, _: &serde_json::Value| {
            ran.lock().unwrap().push(name.to_string());
            r#"{"successful":true}"#.to_string()
        }
    }

    /// "Open Safari, make a note and add a reminder": the note and the reminder
    /// run while the app waits for the screen, rather than in line behind it.
    #[tokio::test]
    async fn independent_calls_in_one_round_run_concurrently() {
        let round = [
            "ALOUD_OPEN_APP",
            "ALOUD_NOTES_CREATE",
            "ALOUD_REMINDERS_ADD",
        ];
        let (brain, _) = fake_brain(vec![calls(&round), content("Done."), content("FINISHED")]);
        let tools = round.map(|name| tool(name, false));
        let ran = std::sync::Mutex::new(Vec::new());
        let mock = logging(&ran);
        let brains = [brain];

        let (answer, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                play(
                    &brains,
                    "open safari, note and remind",
                    &tools,
                    &mock,
                    |_| {}
                ),
                hold_the_screen_until(&ran, 2),
            )
        })
        .await
        .expect("the note and reminder queued behind the app");
        answer.unwrap();
        assert_eq!(
            *ran.lock().unwrap(),
            [
                "ALOUD_NOTES_CREATE",
                "ALOUD_REMINDERS_ADD",
                "ALOUD_OPEN_APP"
            ]
        );
    }

    /// A screen run keeps the screen between its rounds — here while its click
    /// waits on the user — and another run's app does not open until it ends.
    #[tokio::test]
    async fn ui_tools_never_run_at_the_same_time() {
        let (screen_brain, _) = fake_brain(vec![
            calls(&[aloud_screen::READ]),
            calls(&[aloud_screen::CLICK]),
            content("Clicked."),
            content("FINISHED"),
        ]);
        let (other_brain, _) = fake_brain(vec![
            calls(&["ALOUD_OPEN_APP"]),
            content("Opened."),
            content("FINISHED"),
        ]);
        let tools = [
            tool(aloud_screen::READ, true),
            tool(aloud_screen::CLICK, false),
            tool("ALOUD_OPEN_APP", false),
        ];
        let ran = std::sync::Mutex::new(Vec::new());
        let mock = logging(&ran);
        let (gate_up, answered, other_go) = (
            tokio::sync::Notify::new(),
            tokio::sync::Notify::new(),
            tokio::sync::Notify::new(),
        );

        let (screen_brains, composio) = ([screen_brain], ComposioClient::new("").unwrap());
        let screen_run = run(
            &screen_brains,
            &composio,
            command("click the button", &tools),
            Callbacks {
                on_event: |_| {},
                confirm: |call: ProposedCall| {
                    let (gate_up, answered) = (&gate_up, &answered);
                    async move {
                        gate_up.notify_one();
                        answered.notified().await;
                        Verdict::Run(call.args)
                    }
                },
                ask: |_: String| async { None },
                cancelled: || false,
                mock: Some(&mock),
            },
        );
        let other_run = async {
            other_go.notified().await;
            play(&[other_brain], "open safari", &tools, &mock, |_| {}).await
        };
        let user = async {
            gate_up.notified().await;
            other_go.notify_one();
            // Time for the other run to reach the screen, were it free.
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(*ran.lock().unwrap(), [aloud_screen::READ]);
            answered.notify_one();
        };

        let (screen, other, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(screen_run, other_run, user)
        })
        .await
        .expect("deadlocked");
        screen.unwrap();
        other.unwrap();
        assert_eq!(
            *ran.lock().unwrap(),
            [aloud_screen::READ, aloud_screen::CLICK, "ALOUD_OPEN_APP"]
        );
    }

    /// The app is held up behind the screen, so the inbox read finishes first.
    /// The model and the panel still get the app's result first, each matched
    /// to its own call.
    #[tokio::test]
    async fn results_are_reported_in_call_order() {
        let round = ["ALOUD_OPEN_APP", "GMAIL_FETCH_EMAILS"];
        let (brain, requests) = fake_brain(vec![
            calls(&round),
            content("Opened it."),
            content("FINISHED"),
        ]);
        let tools = [tool(round[0], false), tool(round[1], true)];
        let ran = std::sync::Mutex::new(Vec::new());
        let mock = |name: &str, _: &serde_json::Value| {
            ran.lock().unwrap().push(name.to_string());
            serde_json::json!({ "successful": true, "data": name }).to_string()
        };
        let mut finished = Vec::new();
        let brains = [brain];

        let (answer, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                play(
                    &brains,
                    "open safari and check my mail",
                    &tools,
                    &mock,
                    |e| {
                        if let LoopEvent::ToolCallResult { step, name, .. } = e {
                            finished.push((step, name));
                        }
                    }
                ),
                hold_the_screen_until(&ran, 1),
            )
        })
        .await
        .expect("the read queued behind the app");
        answer.unwrap();

        assert_eq!(
            *ran.lock().unwrap(),
            [round[1], round[0]],
            "the read ran first"
        );
        assert_eq!(
            finished,
            [(0, round[0].to_string()), (1, round[1].to_string())]
        );
        let next: serde_json::Value = serde_json::from_str(&requests.lock().unwrap()[1]).unwrap();
        let results: Vec<(&str, &str)> = next["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| {
                (
                    m["tool_call_id"].as_str().unwrap(),
                    m["content"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "0");
        assert!(results[0].1.contains(round[0]), "{results:?}");
        assert_eq!(results[1].0, "1");
        assert!(results[1].1.contains(round[1]), "{results:?}");
    }

    /// Another command's 429 left the only brain cooling for a second: the
    /// run waits it out before asking, rather than spending a request to be
    /// told again.
    #[tokio::test]
    async fn a_cooled_down_model_waits_before_the_next_call() {
        let (brain, requests) =
            fake_brain(vec![rate_limited(1), content("Hi."), content("FINISHED")]);
        let earlier = brain
            .client
            .chat_with_tools(brain.model, &[Message::user("hi")], None, None)
            .await;
        assert!(earlier.is_err(), "the 429 that starts the cooldown");
        let mock = |_: &str, _: &serde_json::Value| String::new();
        let mut waits = Vec::new();
        let started = std::time::Instant::now();

        let answer = play(std::slice::from_ref(&brain), "say hi", &[], &mock, |e| {
            if let LoopEvent::RateLimited { seconds } = e {
                waits.push((seconds, requests.lock().unwrap().len()));
            }
        })
        .await
        .unwrap();

        assert_eq!(answer, "Hi.");
        assert_eq!(waits, [(1, 1)], "waited before its first request");
        assert!(started.elapsed() >= Duration::from_millis(900));
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    /// With another brain to hand, a model known to be out of budget is not
    /// asked at all: the next command goes straight past it.
    #[tokio::test]
    async fn a_cooled_down_brain_hands_the_call_to_the_next_without_a_request() {
        let (first, first_requests) = fake_brain(vec![rate_limited(60)]);
        let (second, _) = fake_brain(vec![
            content("Hi."),
            content("FINISHED"),
            content("Hi again."),
            content("FINISHED"),
        ]);
        let brains = [first, second];
        let mock = |_: &str, _: &serde_json::Value| String::new();

        let one = play(&brains, "say hi", &[], &mock, |_| {}).await.unwrap();
        let two = play(&brains, "say hi again", &[], &mock, |_| {})
            .await
            .unwrap();

        assert_eq!((one.as_str(), two.as_str()), ("Hi.", "Hi again."));
        assert_eq!(
            first_requests.lock().unwrap().len(),
            1,
            "only the 429 itself reached the cooled brain"
        );
    }

    /// NVIDIA's free endpoint sometimes accepts a request and never answers.
    /// The run must move to the next brain when the turn times out, not fail.
    #[tokio::test]
    async fn a_brain_that_times_out_hands_the_command_to_the_next() {
        let hung = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let silent = Brain {
            client: aloud_llm::LlmClient::new(
                "test",
                Some(&format!("http://{}", hung.local_addr().unwrap())),
            )
            .unwrap()
            .with_tool_timeout(Duration::from_millis(200)),
            model: "fake",
        };
        let (next, _) = fake_brain(vec![content("It's sunny."), content("FINISHED")]);
        let mock = |_: &str, _: &serde_json::Value| String::new();
        let started = std::time::Instant::now();

        let answer = play(&[silent, next], "what's the weather", &[], &mock, |_| {})
            .await
            .unwrap();

        assert_eq!(answer, "It's sunny.");
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(hung);
    }

    /// Plays `replies` through the loop with every tool mocked and every gate
    /// approved, returning the gates `confirm` was asked about.
    async fn gates_for(replies: Vec<serde_json::Value>, tools: &[ToolDef]) -> Vec<Option<Gate>> {
        let (brain, _) = fake_brain(replies);
        let mock = |_: &str, _: &serde_json::Value| r#"{"successful":true}"#.to_string();
        let gated = std::sync::Mutex::new(Vec::new());
        run(
            &[brain],
            &ComposioClient::new("").unwrap(),
            Command {
                said: "summarise my inbox",
                prompt: "summarise my inbox",
                tools,
                more: &[],
                front: None,
                document: None,
                project: None,
                selection: false,
            },
            Callbacks {
                on_event: |_| {},
                confirm: |call: ProposedCall| {
                    gated.lock().unwrap().push(call.gate);
                    async move { Verdict::Run(call.args) }
                },
                ask: |_: String| async { None },
                cancelled: || false,
                mock: Some(&mock),
            },
        )
        .await
        .unwrap();
        gated.into_inner().unwrap()
    }

    /// The caller decides from the saved grant, so the loop must hand it the
    /// key and the named-app rule. A send chosen alongside the inbox read was
    /// chosen before the inbox came back, so nothing in it could have asked
    /// for it. The next round's send was chosen after, so it asks as outside
    /// content's; a yes clears Slack for the rest of the run.
    #[tokio::test]
    async fn a_gated_write_reaches_confirm_with_its_gate() {
        let replies = vec![
            calls(&["GMAIL_FETCH_EMAILS", "SLACK_SEND_MESSAGE"]),
            calls(&["SLACK_SEND_MESSAGE"]),
            calls(&["SLACK_SEND_MESSAGE"]),
            content("Posted the summary."),
            content("FINISHED"),
        ];
        let tools = [
            tool("GMAIL_FETCH_EMAILS", true),
            tool("SLACK_SEND_MESSAGE", false),
        ];
        let gates = gates_for(replies, &tools).await;
        assert_eq!(gates.len(), 3, "each send asks the caller: {gates:?}");
        let first = gates[0].as_ref().expect("a write carries its gate");
        assert_eq!(first.key, "slack:send-and-share");
        assert!(!first.named_rule, "chosen before the inbox returned");
        let second = gates[1].as_ref().unwrap();
        assert!(second.named_rule, "the inbox read was outside content");
        assert!(second.reason.is_some());
        assert!(
            !gates[2].as_ref().unwrap().named_rule,
            "a yes cleared Slack"
        );
    }

    #[tokio::test]
    async fn reads_and_engine_tools_never_reach_the_gate() {
        let replies = vec![
            calls(&[
                "GMAIL_FETCH_EMAILS",
                aloud_engine::BROWSER_TASK,
                aloud_screen::READ,
            ]),
            content("Here is the summary."),
            content("FINISHED"),
        ];
        let tools = [
            tool("GMAIL_FETCH_EMAILS", true),
            tool(aloud_engine::BROWSER_TASK, false),
            tool(aloud_screen::READ, true),
        ];
        assert_eq!(gates_for(replies, &tools).await, Vec::new());
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

    /// Only reads hit `MAX_CALLS_PER_TOOL`. The old line said every tool gets
    /// cut off, which was false for writes and told the model a repeat could
    /// never finish.
    #[test]
    fn the_prompt_no_longer_says_repeated_writes_get_cut_off() {
        assert!(!TOOL_SYSTEM_PROMPT.contains("cut off"));
        assert!(TOOL_SYSTEM_PROMPT.contains("pass `times`"));
        assert!(TOOL_SYSTEM_PROMPT.contains(
            "When a command has independent steps, call all of their tools together in one reply."
        ));
    }

    #[test]
    fn messaging_goes_through_its_tools_by_name_not_the_screen() {
        assert!(TOOL_SYSTEM_PROMPT
            .contains("Never use screen control to message someone when a messaging tool exists."));
        assert!(TOOL_SYSTEM_PROMPT.contains("pass the name straight in"));
    }

    /// A mocked eval run must not read the real Contacts, or a case's outcome
    /// would depend on whose Mac it ran on.
    #[tokio::test]
    async fn an_eval_run_never_looks_up_contacts() {
        let mock = |_: &str, _: &serde_json::Value| String::new();
        let to_a_name = serde_json::json!({ "to": "Suhani", "text": "hi" });
        assert!(prepare("ALOUD_MESSAGES_SEND", &to_a_name, Some(&mock))
            .await
            .is_none());
        // Unmocked, it is wired: a handle comes back as it went in.
        let to_a_handle = serde_json::json!({ "to": "+15550001111", "text": "hi" });
        assert_eq!(
            prepare("ALOUD_MESSAGES_SEND", &to_a_handle, None).await,
            Some(Ok(to_a_handle.clone()))
        );
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
    fn only_our_own_writes_can_end_a_run_early() {
        let tools = [
            tool("ALOUD_NOTES_CREATE", false),
            tool("ALOUD_FINDER_SEARCH", true),
            tool("ALOUD_SPOTIFY", false),
            tool("GMAIL_SEND_EMAIL", false),
            tool(aloud_screen::CLICK, false),
        ];
        let none = serde_json::json!({});
        assert!(native_write("ALOUD_NOTES_CREATE", &none, &tools));
        assert!(!native_write("ALOUD_FINDER_SEARCH", &none, &tools));
        assert!(!native_write(
            "ALOUD_SPOTIFY",
            &serde_json::json!({ "action": "now_playing" }),
            &tools
        ));
        assert!(native_write(
            "ALOUD_SPOTIFY",
            &serde_json::json!({ "action": "pause" }),
            &tools
        ));
        assert!(!native_write("GMAIL_SEND_EMAIL", &none, &tools));
        assert!(!native_write(ASK_USER, &none, &tools));
        assert!(!native_write("ALOUD_BROWSER_TASK", &none, &tools));
        // One click of several: ending the run on it would stop mid-task.
        assert!(!native_write(aloud_screen::CLICK, &none, &tools));
    }

    /// One yes covers a screen run: the first action asks, the rest do not,
    /// and a no re-arms it (the loop's decline arm clears the flag).
    #[test]
    fn screen_control_asks_once_per_run() {
        let mut approved = false;
        assert!(
            screen_gate(&mut approved),
            "the first screen action must ask"
        );
        assert!(
            !screen_gate(&mut approved),
            "a yes covers the rest of the run"
        );
        approved = false;
        assert!(screen_gate(&mut approved), "a no asks again next time");
    }

    /// Groq's free tier trips on a command's second step; Gemini takes over,
    /// but only if the calls Groq made carry a signature Gemini accepts. One it
    /// signed itself must be left as it is.
    #[test]
    fn a_switch_to_gemini_mid_run_signs_the_calls_it_never_made() {
        let call = |id: &str, extra: Option<serde_json::Value>| aloud_llm::ToolCall {
            id: id.to_string(),
            kind: "function".to_string(),
            function: aloud_llm::FunctionCall {
                name: "ALOUD_NOTES_CREATE".to_string(),
                arguments: "{}".to_string(),
            },
            extra_content: extra,
        };
        let own = serde_json::json!({ "google": { "thought_signature": "real" } });
        let mut messages = vec![
            Message::user("make a note"),
            Message {
                role: aloud_llm::Role::Assistant,
                content: None,
                tool_calls: Some(vec![call("groq", None), call("gemini", Some(own.clone()))]),
                tool_call_id: None,
            },
        ];
        sign_for_gemini(&mut messages);
        let calls = messages[1].tool_calls.as_ref().unwrap();
        assert_eq!(
            calls[0].extra_content.as_ref().unwrap()["google"]["thought_signature"],
            "skip_thought_signature_validator"
        );
        assert_eq!(calls[1].extra_content.as_ref(), Some(&own));
    }

    /// "take 5 photos" ended after one: the round's single write confirmed
    /// itself and the run stopped. A command naming several steps goes back to
    /// the model instead.
    #[test]
    fn a_command_naming_several_actions_does_not_finish_after_one_round() {
        for said in [
            "take 5 photos",
            "open photo booth and take a photo",
            "create a note then set volume to 20",
            "press tab 4 times",
            "skip three songs",
            "ping me twice",
        ] {
            assert!(names_several_actions(said), "{said}");
        }
        for said in ["create a note called groceries", "take a photo", "pause"] {
            assert!(!names_several_actions(said), "{said}");
        }
    }

    #[test]
    fn a_time_of_day_is_not_a_count() {
        for said in [
            "remind me to call Ana at 5",
            "remind me in 10 minutes to stretch",
            "set the volume to 30",
            "remind me at 5pm",
        ] {
            assert!(!names_several_actions(said), "{said}");
        }
    }

    /// A round with no text, or with a failed or foreign call, goes back to the
    /// model as before.
    #[test]
    fn an_early_finish_needs_the_models_own_words_and_a_clean_round() {
        let none: [Option<String>; 0] = [];
        assert_eq!(
            finishes_early(Some(" Created the note. "), true, &none).as_deref(),
            Some("Created the note.")
        );
        assert_eq!(
            finishes_early(Some("Created the note."), false, &none),
            None
        );
        assert_eq!(finishes_early(Some("  "), true, &none), None);
        assert_eq!(finishes_early(None, true, &none), None);
        // No words from the model, but every write said what it did.
        let own = [Some("Created the note “Cake”.".to_string())];
        assert_eq!(
            finishes_early(None, true, &own).as_deref(),
            Some("Created the note “Cake”.")
        );
        // One write that can't phrase itself sends the round back to the model.
        let partly = [Some("Paused Spotify.".to_string()), None];
        assert_eq!(finishes_early(None, true, &partly), None);
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
