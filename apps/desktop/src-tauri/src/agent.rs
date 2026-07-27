//! Agent mode: run a spoken command across apps.
//!
//!   transcribe → parse intent → (confirm if it writes) → dispatch → report
//!
//! An action never fires without explicit approval: reads run straight through,
//! writes surface a confirmation and wait for a yes. Drawn in the overlay's
//! drop-down panel, like Ask, but this one collects a decision — hence the
//! one-shot channel the approve/reject commands resolve.

use aloud_store::Settings;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::oneshot;
use tracing::Instrument;

/// The window these events go to — the overlay's, not one of Agent's own.
const LABEL: &str = "overlay";
/// How long a write waits for a yes before giving up and declining.
const CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The pending confirmation's reply channel, so approve/reject can wake the
/// awaiting dispatch. Only one is ever in flight; a new command replaces it.
///
/// The original arguments ride along because the panel only ever sees the
/// flattened strings `display_args` produced. Merging its edits back into the
/// real nested shape needs the thing that was flattened.
#[derive(Default)]
pub struct PendingConfirmation(Mutex<Option<Pending>>);

struct Pending {
    reply: oneshot::Sender<Option<serde_json::Value>>,
    args: serde_json::Value,
}

/// The pending question's reply channel. Separate from the confirmation above:
/// a run can be waiting on either, never both, and conflating them would let a
/// Confirm click answer a question.
#[derive(Default)]
pub struct PendingQuestion(Mutex<Option<oneshot::Sender<Option<String>>>>);

/// Streamed to the agent panel. Names follow the AG-UI protocol's tool-call
/// lifecycle (https://docs.ag-ui.com) so the shape maps onto AI Elements' tool
/// renderer — the vocabulary only, not the wire protocol.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[tauri_specta(event_name = "agent")]
pub enum AgentEvent {
    /// The transcript, echoed so the panel shows what it heard.
    RunStarted(String),
    /// The batch a round intends to run, sent before the first call so a run
    /// about to send mail can still be read. Only when a round holds >1 call.
    Planned(Vec<PlannedCall>),
    /// A parsed action awaiting dispatch. A write pauses here for confirmation.
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    /// The model needs something only the user can give. The run is suspended
    /// until `answer_agent`; the panel shows an input rather than a gate.
    Asked(Question),
    /// A transient status line (e.g. "rate limited, retrying in 30s") shown
    /// while the run is otherwise silent; cleared by the next event.
    Status(String),
    RunFinished(String),
    RunError(String),
}

/// A tool the run intends to call. No arguments: they arrive with the call
/// itself, which is where it stops being a plan and starts being an action.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCall {
    pub name: String,
    pub app: String,
}

/// A tool call for display. Kept separate from `ParsedAction` so the UI
/// contract does not leak parser internals.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// e.g. "github.create_issue".
    pub name: String,
    /// The target app, e.g. "github".
    pub app: String,
    /// Args as strings, not `serde_json::Value`: specta refuses to export its
    /// i64-backed number type. Non-string values keep their JSON form.
    pub args: std::collections::BTreeMap<String, String>,
    /// Whether this call pauses for confirmation before it runs (every write).
    pub requires_confirmation: bool,
}

/// A question put to the user mid-run. `options` non-empty means the answer is
/// one of them, so the panel offers buttons instead of a text field.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
}

fn emit(app: &AppHandle, event: AgentEvent) {
    if let Err(e) = event.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send to agent panel");
    }
}

/// Runs the full agent flow for a transcript. Called by the pipeline, which
/// owns `cancelled` — the loop polls it between tool calls so Escape stops a
/// multi-tool round part-way instead of after all of it.
pub async fn run(
    app: &AppHandle,
    transcript: String,
    settings: &Settings,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    crate::overlay::show_panel(app);
    emit(app, AgentEvent::RunStarted(transcript.clone()));

    match execute(app, &transcript, settings, cancelled).await {
        Ok(message) => emit(app, AgentEvent::RunFinished(message)),
        // A cancel is the user's own doing: close the panel rather than
        // showing them an error for the thing they just asked to stop.
        Err(e) if is_cancelled(&e) => {
            tracing::info!("agent cancelled");
            hide(app);
        }
        Err(e) => {
            tracing::error!(%e, "agent failed");
            emit(app, AgentEvent::RunError(format!("{e:#}")));
        }
    }
    Ok(())
}

/// Whether a failure is the loop's own cancellation, which is not an error to
/// report. Matched on the message because the loop returns `anyhow::Error` and
/// a typed error for one string is not worth the crate boundary.
fn is_cancelled(error: &anyhow::Error) -> bool {
    error.to_string() == "cancelled"
}

// Gemini, not Groq: a multi-round tool loop makes several chat calls in a row
// and Groq's tokens-per-minute limit throttles it mid-command. The endpoint is
// OpenAI-compatible, so the same LlmClient works unchanged.
const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";

#[tracing::instrument(
    name = "agent_run",
    skip_all,
    fields(tool_count = tracing::field::Empty)
)]
async fn execute(
    app: &AppHandle,
    transcript: &str,
    settings: &Settings,
    cancelled: impl Fn() -> bool,
) -> Result<String> {
    if settings.gemini_api_key.trim().is_empty() {
        bail!("Agent mode is unavailable — no Gemini credential is configured");
    }
    if settings.composio_api_key.trim().is_empty() {
        bail!("Agent mode is unavailable — no Composio credential is configured");
    }

    let llm = aloud_llm::LlmClient::new(&settings.gemini_api_key, Some(GEMINI_BASE_URL))?;
    let composio = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;

    // A Composio round-trip on every command, before the model is even asked —
    // invisible in any per-tool timing, so it gets its own span.
    let mut tools = composio
        .available_tools(transcript)
        .instrument(tracing::info_span!(
            "tool_discovery",
            count = tracing::field::Empty
        ))
        .await?;

    // The local half: native macOS actions for apps that are installed, the
    // same ToolDef shape so the loop dispatches them with one branch. Composio
    // may be unconnected and native tools still make the command runnable, so
    // the "nothing to do" bail moves below the concat.
    let installed = aloud_selection::installed_apps();
    tools.extend(
        aloud_native::tools(&installed)
            .into_iter()
            .map(|t| aloud_agent::ToolDef {
                name: t.name,
                toolkit: "macOS".to_string(),
                description: t.description,
                parameters: t.parameters,
                read_only: t.read_only,
            }),
    );

    // Layer 2: driving the focused app by its real accessibility elements, for
    // everything no named tool covers. Empty when the helpers are not in this
    // build, so the tool list simply lacks them rather than offering tools that
    // can only fail.
    tools.extend(
        aloud_computer::tools()
            .into_iter()
            .map(|t| aloud_agent::ToolDef {
                name: t.name,
                toolkit: "Screen".to_string(),
                description: t.description,
                parameters: t.parameters,
                read_only: t.read_only,
            }),
    );

    tracing::Span::current().record("tool_count", tools.len());
    if tools.is_empty() {
        bail!("No app is connected in Composio yet");
    }

    aloud_agent::run_tool_loop(
        &llm,
        GEMINI_MODEL,
        &composio,
        transcript,
        &tools,
        |event| emit(app, event_of(event)),
        // The safety gate. The offered tools carry their toolkit, so it names
        // the real app rather than deriving one from the slug.
        |name, args| {
            let toolkit = tools
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.toolkit.clone())
                .unwrap_or_else(|| tool_app(&name));
            confirm_write(app, name, toolkit, args)
        },
        // The question itself already went out through `on_event`; this only
        // opens the reply channel and waits on it.
        |_question| await_answer(app),
        cancelled,
    )
    .await
}

/// Waits for the panel's reply to the question `event_of` has already emitted.
async fn await_answer(app: &AppHandle) -> Option<String> {
    let (tx, rx) = oneshot::channel();
    if let Some(state) = app.try_state::<PendingQuestion>() {
        *state.0.lock().unwrap() = Some(tx);
    } else {
        tracing::error!("PendingQuestion state missing");
        return None;
    }
    // The same ceiling as a confirmation, and for the same reason: a run that
    // can wait forever is a run that wedges the agent.
    match tokio::time::timeout(CONFIRM_TIMEOUT, rx).await {
        Ok(answer) => answer.unwrap_or(None),
        Err(_) => {
            tracing::warn!("question timed out; treating as unanswered");
            None
        }
    }
}

/// Resolves the pending question. `None` is the user dismissing it, which the
/// loop reports to the model as unanswered rather than as an error.
#[tauri::command]
#[specta::specta]
pub fn answer_agent(app: AppHandle, answer: Option<String>) {
    if let Some(state) = app.try_state::<PendingQuestion>() {
        if let Some(tx) = state.0.lock().unwrap().take() {
            let _ = tx.send(answer.filter(|text| !text.trim().is_empty()));
        }
    }
}

/// Surfaces a write tool for confirmation and awaits the panel's decision. Takes
/// owned args so the returned future doesn't borrow the loop's transient data.
async fn confirm_write(
    app: &AppHandle,
    name: String,
    toolkit: String,
    args: serde_json::Value,
) -> Option<serde_json::Value> {
    emit(
        app,
        AgentEvent::ToolCall(ToolCall {
            app: toolkit,
            name,
            args: display_args(&args),
            requires_confirmation: true,
        }),
    );
    await_confirmation(app, args).await
}

/// Flattens a tool's argument object into displayable string fields.
fn display_args(args: &serde_json::Value) -> std::collections::BTreeMap<String, String> {
    args.as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    let text = match value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (key.clone(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn event_of(event: aloud_agent::LoopEvent) -> AgentEvent {
    match event {
        aloud_agent::LoopEvent::Planned { names } => AgentEvent::Planned(
            names
                .into_iter()
                .map(|name| PlannedCall {
                    app: tool_app(&name),
                    name,
                })
                .collect(),
        ),
        // The ask tool is not a tool as far as the panel is concerned — the
        // question is the UI, and a "Aloud: ask user" row beside it reads as
        // plumbing leaking through.
        aloud_agent::LoopEvent::Asked { question, options } => {
            AgentEvent::Asked(Question { question, options })
        }
        aloud_agent::LoopEvent::ToolCallStart { name, args } => AgentEvent::ToolCall(ToolCall {
            app: tool_app(&name),
            name,
            args: display_args(&args),
            // The gate is enforced in the loop; the panel just shows the call.
            requires_confirmation: false,
        }),
        aloud_agent::LoopEvent::ToolCallResult { name, success } => {
            AgentEvent::ToolResult(ToolResult {
                success,
                message: if success {
                    format!("{name} done")
                } else {
                    format!("{name} failed")
                },
            })
        }
        aloud_agent::LoopEvent::RateLimited { seconds } => {
            AgentEvent::Status(format!("Rate limited — retrying in {seconds}s…"))
        }
    }
}

/// The toolkit a Composio tool slug belongs to, for display.
fn tool_app(slug: &str) -> String {
    aloud_agent::toolkit_of_slug(slug)
}

/// Blocks until the panel approves or rejects. A dropped sender (panel closed,
/// new command started) or a timeout reads as a rejection — a gate that can
/// hang wedges the whole agent, and declining fails in the safe direction.
async fn await_confirmation(app: &AppHandle, args: serde_json::Value) -> Option<serde_json::Value> {
    let (tx, rx) = oneshot::channel();
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        let mut slot = state.0.lock().unwrap();
        // Replacing a live confirmation drops its sender, which resolves the
        // waiting dispatch as "declined" without the user ever seeing a prompt.
        // The panel renders one gate at a time, so a second one here means two
        // writes are racing; refusing the newcomer is the honest answer, and it
        // keeps the visible prompt the one that is actually answerable.
        if slot.as_ref().is_some_and(|p| !p.reply.is_closed()) {
            tracing::warn!("a confirmation is already open; declining the second");
            return None;
        }
        *slot = Some(Pending { reply: tx, args });
    } else {
        tracing::error!("PendingConfirmation state missing");
        return None;
    }
    match tokio::time::timeout(CONFIRM_TIMEOUT, rx).await {
        Ok(answer) => answer.unwrap_or(None),
        Err(_) => {
            tracing::warn!("confirmation timed out; treating as declined");
            None
        }
    }
}

/// Resolves the pending confirmation. Called by the panel's Cancel/Confirm
/// buttons; `edits` carries any field the user filled in or corrected before
/// approving, in the same flattened form the panel was given.
#[tauri::command]
#[specta::specta]
pub fn confirm_agent(
    app: AppHandle,
    approved: bool,
    edits: Option<std::collections::BTreeMap<String, String>>,
) {
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        if let Some(pending) = state.0.lock().unwrap().take() {
            let answer = approved.then(|| match edits {
                Some(edits) => apply_edits(&pending.args, &edits),
                None => pending.args.clone(),
            });
            // A gone receiver means the flow was already abandoned.
            let _ = pending.reply.send(answer);
        }
    }
    if !approved {
        hide(&app);
    }
}

/// Folds the panel's edited strings back into a tool's real arguments.
///
/// The inverse of [`display_args`]: a field that was a string stays one, and a
/// field that was flattened to JSON text is parsed back to the shape the tool
/// expects. A value that no longer parses is kept as a string rather than
/// dropped — a tool rejecting a bad argument says so, where a silently missing
/// one just does the wrong thing.
fn apply_edits(
    original: &serde_json::Value,
    edits: &std::collections::BTreeMap<String, String>,
) -> serde_json::Value {
    let mut out = original.as_object().cloned().unwrap_or_default();
    for (key, edited) in edits {
        let value = match original.get(key) {
            Some(serde_json::Value::String(_)) | None => serde_json::Value::String(edited.clone()),
            Some(_) => serde_json::from_str(edited)
                .unwrap_or_else(|_| serde_json::Value::String(edited.clone())),
        };
        out.insert(key.clone(), value);
    }
    serde_json::Value::Object(out)
}

pub fn hide(app: &AppHandle) {
    crate::overlay::end_panel(app);
}

/// Lets the panel dismiss itself on Escape or after showing a result.
#[tauri::command]
#[specta::specta]
pub fn close_agent(app: AppHandle) {
    // Abandoning a pending confirmation reads as a rejection; the write never
    // fires.
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        state.0.lock().unwrap().take();
    }
    // A dropped sender resolves the question as unanswered, so a closed panel
    // never leaves the loop waiting out its full timeout.
    if let Some(state) = app.try_state::<PendingQuestion>() {
        state.0.lock().unwrap().take();
    }
    hide(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The panel only ever sees `display_args`' flat strings, so approving a
    /// gate has to rebuild the nested shape the tool expects. Getting this
    /// wrong writes a contact with the address as a quoted JSON string, or
    /// drops it entirely — both of which look like a successful save.
    #[test]
    fn edits_fold_back_into_the_shape_the_tool_expects() {
        let original = json!({
            "names": [{ "givenName": "Dad" }],
            "emailAddresses": [],
            "personFields": "names,emailAddresses",
        });
        let edits = [
            (
                "emailAddresses".to_string(),
                r#"[{"value":"dad@example.com"}]"#.to_string(),
            ),
            (
                "personFields".to_string(),
                "names,emailAddresses".to_string(),
            ),
        ]
        .into_iter()
        .collect();

        let merged = apply_edits(&original, &edits);
        assert_eq!(merged["emailAddresses"][0]["value"], "dad@example.com");
        // A string field stays a string rather than being parsed into a number
        // or a bool that happens to look like one.
        assert_eq!(merged["personFields"], "names,emailAddresses");
        // Untouched keys survive.
        assert_eq!(merged["names"][0]["givenName"], "Dad");
    }

    #[test]
    fn an_edit_that_stops_being_json_is_kept_not_dropped() {
        let original = json!({ "emailAddresses": [{ "value": "a@b.c" }] });
        let edits = [("emailAddresses".to_string(), "not json".to_string())]
            .into_iter()
            .collect();
        assert_eq!(apply_edits(&original, &edits)["emailAddresses"], "not json");
    }

    /// An empty or whitespace answer is the user having dismissed the box, not
    /// an answer of "". Sending it as one would have the model act on a blank.
    #[test]
    fn a_blank_reply_is_not_an_answer() {
        assert_eq!(
            Some("  ".to_string()).filter(|t| !t.trim().is_empty()),
            None
        );
        assert_eq!(
            Some(" a@b.c ".to_string()).filter(|t| !t.trim().is_empty()),
            Some(" a@b.c ".to_string())
        );
    }

    #[test]
    fn a_field_the_tool_never_had_arrives_as_a_string() {
        let edits = [("subject".to_string(), "Hello".to_string())]
            .into_iter()
            .collect();
        assert_eq!(apply_edits(&json!({}), &edits)["subject"], "Hello");
    }
}
