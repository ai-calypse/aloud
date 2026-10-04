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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::oneshot;

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

/// Which run the panel is currently showing.
///
/// A run is only stopped *between* tools — the pipeline polls its cancel flag
/// rather than dropping the future, so a superseded run keeps executing its
/// current tool and then emits into a panel the next run has already reset.
/// That is how a new command ended up showing the previous one's steps. Every
/// run takes a ticket here and its events are dropped once it is no longer the
/// holder, so a late event from an abandoned run cannot reach the panel.
static CURRENT_RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Claims the panel for a new run, returning its ticket.
fn claim_run() -> u64 {
    CURRENT_RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

fn is_current(run: u64) -> bool {
    CURRENT_RUN.load(std::sync::atomic::Ordering::SeqCst) == run
}

/// One run's claim on the panel and where its events go.
struct Run {
    ticket: u64,
    /// Reporting nowhere but the wings. An always-listening "create a note"
    /// needs no panel; a run that has to ask, or has something to read, is
    /// [`Run::surface`]d.
    quiet: AtomicBool,
    /// Whether any tool ran. A quiet run that only answered still has to show
    /// the answer somewhere bigger than the bar's one line.
    acted: AtomicBool,
    transcript: String,
}

impl Run {
    /// Emits only while this run still owns the panel. A superseded run keeps
    /// executing until its current tool returns, and without this its results
    /// land in the panel belonging to the command the user has since spoken.
    fn emit(&self, app: &AppHandle, event: AgentEvent) {
        if !is_current(self.ticket) {
            tracing::debug!(
                ?event,
                run = self.ticket,
                "dropping an event from a superseded run"
            );
            return;
        }
        if self.quiet.load(Ordering::SeqCst) {
            return;
        }
        if let Err(e) = event.emit_to(app, LABEL) {
            tracing::warn!(%e, "could not send to agent panel");
        }
    }

    /// Opens the panel on a quiet run, re-sending what was asked so it opens
    /// on the request rather than on a question with no context.
    fn surface(&self, app: &AppHandle) {
        if is_current(self.ticket) && self.quiet.swap(false, Ordering::SeqCst) {
            crate::overlay::show_panel(app);
            self.emit(app, AgentEvent::RunStarted(self.transcript.clone()));
        }
    }
}

/// What the user was looking at when they spoke, read before our panel took focus.
#[derive(Clone, Copy)]
struct Context<'a> {
    front: Option<&'a str>,
    /// The front window's title and file.
    title: Option<&'a str>,
    document: Option<&'a str>,
    /// The repo an editor's title names, for when it gives no file; see
    /// `aloud_native::editor_project`.
    project: Option<&'a str>,
    selection: Option<&'a str>,
}

/// Whether a finished run's message needs the panel: an answer with no action
/// behind it, or one longer than a line ("Created the note" can go unsaid;
/// an email summary cannot).
fn needs_reading(acted: bool, message: &str) -> bool {
    !acted || message.contains('\n') || message.chars().count() > 100
}

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
    /// What the gate's card shows, read from the real arguments. Only on the
    /// gate event; the start event and reads carry none. Boxed so every other
    /// event is not sized to the card.
    pub preview: Option<Box<aloud_agent::preview::ActionPreview>>,
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
    /// What the row's Show button brings back, when the step made something.
    pub show: Option<ShowTarget>,
}

/// A note id in Notes, a saved file, a folder in the app that opened it — for
/// [`show_result`]. The tools work in the background, so this is the way back.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShowTarget {
    pub app: String,
    pub target: String,
}

fn show_of(show: Option<(String, String)>) -> Option<ShowTarget> {
    show.map(|(app, target)| ShowTarget { app, target })
}

/// Runs the full agent flow for a transcript. Called by the pipeline, which
/// owns `cancelled` — the loop polls it between tool calls so Escape stops a
/// multi-tool round part-way instead of after all of it.
///
/// `quiet` (always-listening) keeps the panel closed until the run needs the
/// user or has an answer to show; until then only the listening bar hears it.
pub async fn run(
    app: &AppHandle,
    transcript: String,
    settings: &Settings,
    quiet: bool,
    cancelled: impl Fn() -> bool,
) -> Result<()> {
    // Taking the ticket supersedes any run still in flight: its events stop
    // reaching the panel, and with `cancelled` below it stops at its next tool
    // instead of carrying on unseen.
    let run = Run {
        ticket: claim_run(),
        quiet: AtomicBool::new(quiet),
        acted: AtomicBool::new(false),
        transcript: transcript.clone(),
    };
    let cancelled = || cancelled_or_superseded(run.ticket, &cancelled);

    // A previous run that ended without the panel being dismissed — it finished
    // on its own, errored, or was cancelled — leaves its unanswered gate in the
    // slot. The next command then finds it occupied and declines its own
    // confirmation with no prompt on screen. After the claim, so the run being
    // superseded cannot put a question back in between.
    clear_pending(app);

    // Read before `show_panel`, which focuses our own window and would make us the front app. Our own
    // name means a panel still held focus from the last run, which says nothing about what the user is
    // looking at, so it is dropped.
    let front = aloud_selection::frontmost_app()
        .map(|a| a.name)
        .filter(|name| !name.eq_ignore_ascii_case("aloud"));
    // Also before the panel: once it has focus the window and the selection are ours, not theirs.
    // One bounded task for both, because an app busy on its main thread can hold an accessibility
    // read for seconds.
    let (window, selection) = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        tauri::async_runtime::spawn_blocking(|| {
            (
                aloud_selection::frontmost_window(),
                aloud_selection::selected_text_quietly(),
            )
        }),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    // With no front app the window was our own panel's.
    let (title, document) = window
        .filter(|_| front.is_some())
        .map_or((None, None), |(title, document)| (Some(title), document));
    let project = front
        .as_deref()
        .zip(title.as_deref())
        .and_then(|(app, title)| aloud_native::editor_project(app, title));

    if !quiet {
        crate::overlay::show_panel(app);
    }
    run.emit(app, AgentEvent::RunStarted(transcript.clone()));

    let context = Context {
        front: front.as_deref(),
        title: title.as_deref(),
        document: document.as_deref(),
        project,
        selection: selection.as_deref(),
    };
    match execute(app, &run, &transcript, context, settings, cancelled).await {
        Ok(message) => {
            if needs_reading(run.acted.load(Ordering::SeqCst), &message) {
                run.surface(app);
            }
            run.emit(app, AgentEvent::RunFinished(message));
        }
        // A cancel is the user's own doing: close the panel rather than
        // showing them an error for the thing they just asked to stop.
        Err(e) if is_cancelled(&e) => {
            tracing::info!("agent cancelled");
            // Only if nothing has replaced us — hiding here would otherwise
            // close the panel of the run the user just started.
            if is_current(run.ticket) && !run.quiet.load(Ordering::SeqCst) {
                hide(app);
            }
        }
        Err(e) => {
            tracing::error!(%e, "agent failed");
            // An error is something to read, not a one-line status.
            run.surface(app);
            run.emit(app, AgentEvent::RunError(format!("{e:#}")));
        }
    }
    Ok(())
}

/// A run stops when its caller cancels it or when a newer command has taken
/// the panel. Without the second, a superseded run kept executing reads and
/// writes no one could see.
fn cancelled_or_superseded(ticket: u64, cancelled: &impl Fn() -> bool) -> bool {
    cancelled() || !is_current(ticket)
}

/// Whether a failure is the loop's own cancellation, which is not an error to
/// report. Matched on the message because the loop returns `anyhow::Error` and
/// a typed error for one string is not worth the crate boundary.
fn is_cancelled(error: &anyhow::Error) -> bool {
    error.to_string() == "cancelled"
}

/// Drops any unanswered gate or question. Dropping a sender resolves whoever is
/// waiting on it — as a declined write or an unanswered question — so an
/// abandoned run never leaves a live channel behind.
pub(crate) fn clear_pending(app: &AppHandle) {
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        state.0.lock().unwrap().take();
    }
    if let Some(state) = app.try_state::<PendingQuestion>() {
        state.0.lock().unwrap().take();
    }
}

#[tracing::instrument(
    name = "agent_run",
    skip_all,
    fields(tool_count = tracing::field::Empty, route = tracing::field::Empty)
)]
async fn execute(
    app: &AppHandle,
    run: &Run,
    transcript: &str,
    context: Context<'_>,
    settings: &Settings,
    cancelled: impl Fn() -> bool,
) -> Result<String> {
    let installed = aloud_selection::installed_apps();

    // The fast path, before any language model: "open Slack", "pause", "volume 30" run their tool
    // directly, and "go to youtube", "search lofi on youtube" go straight to the browser engine.
    // Everything else, and a browser engine missing from this build, takes the brain below.
    match aloud_native::fast::route(transcript, &installed) {
        Some(aloud_native::fast::Fast::Tool { name, args, done }) => {
            tracing::Span::current().record("route", "fast");
            let call = aloud_agent::ProposedCall {
                name: name.to_string(),
                args,
                tool: None,
                said: transcript.to_string(),
                names: Default::default(),
            };
            return run_fast(app, run, name, call, done, context).await;
        }
        Some(aloud_native::fast::Fast::Web { query }) => {
            let goal = serde_json::json!({ "goal": transcript, "query": query });
            match aloud_engine::start(aloud_engine::BROWSER_TASK, &goal).await {
                Some(Ok(task)) => {
                    tracing::Span::current().record("route", "web");
                    return run_direct(app, run, transcript, task, &cancelled).await;
                }
                Some(Err(e)) => tracing::warn!(%e, "browser engine unavailable; asking the brain"),
                None => {}
            }
        }
        None => {}
    }

    let brains = aloud_agent::brains(&settings.groq_api_key, &settings.gemini_api_key)?;
    if brains.is_empty() {
        bail!("Agent mode is unavailable — no Groq or Gemini credential is configured");
    }
    // Connected services are optional: without a Composio key the Mac's own tools still run.
    let composio = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;
    let has_composio = !settings.composio_api_key.trim().is_empty();
    let aloud_agent::CommandTools { tools, more } =
        aloud_agent::command_tools(transcript, &installed, has_composio.then_some(&composio)).await;
    tracing::Span::current().record("tool_count", tools.len());
    tracing::Span::current().record("route", "brain");

    let command = aloud_agent::with_context(
        transcript,
        context.front,
        context.title,
        context.document,
        context.selection,
    );

    aloud_agent::run_tool_loop(
        &brains,
        &composio,
        aloud_agent::Command {
            said: transcript,
            prompt: &command,
            tools: &tools,
            more: &more,
            front: context.front,
            document: context.document,
            project: context.project,
        },
        aloud_agent::Callbacks {
            on_event: |event| {
                match &event {
                    aloud_agent::LoopEvent::ToolCallStart { .. } => {
                        run.acted.store(true, Ordering::SeqCst)
                    }
                    // The question goes out through this event before the loop waits on the reply, so
                    // the panel has to be up first.
                    aloud_agent::LoopEvent::Asked { .. } => run.surface(app),
                    _ => {}
                }
                run.emit(app, event_of(event))
            },
            // The safety gate. The call carries its tool from the run's own list,
            // so it names the real app — a service loaded mid-run included —
            // rather than deriving one from the slug.
            confirm: |call: aloud_agent::ProposedCall| {
                let toolkit = call
                    .tool
                    .as_ref()
                    .map(|t| t.toolkit.clone())
                    .unwrap_or_else(|| tool_app(&call.name));
                let schema = call.tool.as_ref().map(|t| t.parameters.clone());
                // No key, no reads: the card names ids from the run's own reads only.
                let composio = has_composio.then_some(&composio);
                async move {
                    match confirm_write(app, run, call, toolkit, schema, composio).await {
                        Some(args) => aloud_agent::Verdict::Run(args),
                        None => aloud_agent::Verdict::Decline,
                    }
                }
            },
            // The question itself already went out through `on_event`; this only
            // opens the reply channel and waits on it.
            ask: |_question| await_answer(app, run),
            cancelled,
            mock: None,
        },
    )
    .await
}

/// Runs one native tool the fast path picked, with the same tool row, gate and result the brain's
/// calls get, and `done` as the answer: no model, so nothing to wait for but the tool itself.
async fn run_fast(
    app: &AppHandle,
    run: &Run,
    name: &'static str,
    call: aloud_agent::ProposedCall,
    done: String,
    context: Context<'_>,
) -> Result<String> {
    run.acted.store(true, Ordering::SeqCst);
    let args = if aloud_native::low_risk(name, &call.args) {
        let args = call.args;
        run.emit(
            app,
            AgentEvent::ToolCall(ToolCall {
                app: "macOS".to_string(),
                name: name.to_string(),
                args: display_args(&args),
                requires_confirmation: false,
                preview: None,
            }),
        );
        args
    } else {
        // Native writes read nothing to name, so no connected services either.
        let schema = aloud_native::params(name);
        match confirm_write(app, run, call, "macOS".to_string(), schema, None).await {
            Some(approved) => approved,
            None => return Ok("Cancelled.".to_string()),
        }
    };
    // After the gate, like the loop's: the row and the confirmation show what was asked.
    let fronted = aloud_native::with_front(&args, context.front, context.document, context.project);
    let outcome =
        tauri::async_runtime::spawn_blocking(move || aloud_native::execute(name, &fronted))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    let (result, show) = match outcome {
        Some(Ok(value)) => (Ok(done), aloud_native::show_target(name, &args, &value)),
        Some(Err(e)) => (Err(anyhow::anyhow!("{e}")), None),
        None => (
            Err(anyhow::anyhow!("{name} is not available on this Mac")),
            None,
        ),
    };
    run.emit(
        app,
        AgentEvent::ToolResult(ToolResult {
            success: result.is_ok(),
            message: format!("{name} {}", if result.is_ok() { "done" } else { "failed" }),
            show: show_of(show),
        }),
    );
    result
}

/// Drives one browser task to the end on the panel: a tool row, each step as the status line, a
/// Yes/No question before a risky click, and the engine's answer as the run's message.
async fn run_direct(
    app: &AppHandle,
    run: &Run,
    transcript: &str,
    mut task: aloud_engine::Task,
    cancelled: &impl Fn() -> bool,
) -> Result<String> {
    let name = aloud_engine::BROWSER_TASK.to_string();
    run.acted.store(true, Ordering::SeqCst);
    run.emit(
        app,
        AgentEvent::ToolCall(ToolCall {
            app: "Browser".to_string(),
            name: name.clone(),
            args: [("goal".to_string(), transcript.to_string())].into(),
            requires_confirmation: false,
            preview: None,
        }),
    );
    loop {
        if cancelled() {
            let _ = task.stop().await;
            bail!("cancelled");
        }
        match task.next().await {
            None => {}
            Some(aloud_engine::Update::Step(text)) => run.emit(app, AgentEvent::Status(text)),
            Some(aloud_engine::Update::NeedsApproval(question)) => {
                run.surface(app);
                run.emit(
                    app,
                    AgentEvent::Asked(Question {
                        question,
                        options: vec!["Yes".to_string(), "No".to_string()],
                    }),
                );
                let yes = matches!(await_answer(app, run).await.as_deref(), Some("Yes"));
                task.approve(yes).await.map_err(anyhow::Error::msg)?;
            }
            Some(aloud_engine::Update::Finished(result)) => {
                let success = result["successful"].as_bool().unwrap_or(false);
                run.emit(
                    app,
                    AgentEvent::ToolResult(ToolResult {
                        success,
                        message: format!("{name} {}", if success { "done" } else { "failed" }),
                        show: None,
                    }),
                );
                if !success {
                    bail!(
                        "{}",
                        result["error"].as_str().unwrap_or("the task didn't finish")
                    );
                }
                // What the page or app showed, else the last thing done.
                let answer: Vec<&str> = result["answer"]
                    .as_array()
                    .map(|lines| lines.iter().filter_map(|l| l.as_str()).take(4).collect())
                    .unwrap_or_default();
                let last = result["steps"]
                    .as_array()
                    .and_then(|s| {
                        s.iter()
                            .rev()
                            .find_map(|l| l.as_str().filter(|l| *l != "Done"))
                    })
                    .unwrap_or("Done");
                return Ok(if answer.is_empty() {
                    last.to_string()
                } else {
                    answer.join(" · ")
                });
            }
        }
    }
}

async fn await_answer(app: &AppHandle, run: &Run) -> Option<String> {
    // A superseded run asking would take the reply slot from the current one.
    if !is_current(run.ticket) {
        return None;
    }
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
/// the call owned so the returned future doesn't borrow the loop's transient data.
/// `schema` is the tool's JSON schema, which the card reads its fields against;
/// `composio` makes the reads the card's mapping names, if any.
async fn confirm_write(
    app: &AppHandle,
    run: &Run,
    call: aloud_agent::ProposedCall,
    toolkit: String,
    schema: Option<serde_json::Value>,
    composio: Option<&aloud_agent::ComposioClient>,
) -> Option<serde_json::Value> {
    // A superseded run must not open a gate: its prompt would appear over the
    // new run's panel, and it would occupy the single confirmation slot the
    // current run needs. Declining is right — the user has moved on.
    if !is_current(run.ticket) {
        tracing::info!(name = %call.name, run = run.ticket, "declining a write from a superseded run");
        return None;
    }
    // A gate needs the panel even on a quiet run: it is a question. Up before
    // the card's reads, so the wait for them is spent on screen.
    run.surface(app);
    let ctx = aloud_agent::preview::Context {
        said: &call.said,
        names: &call.names,
    };
    let mut event = gate_call(
        call.name.clone(),
        toolkit,
        &call.args,
        schema.as_ref(),
        &ctx,
    );
    if let (Some(composio), Some(preview)) = (composio, event.preview.as_deref_mut()) {
        // Each read is cut off inside `enrich` (1.5s, all at once), so a slow
        // service leaves an id unnamed rather than holding the gate back.
        aloud_agent::preview::enrich(preview, &call.name, &call.args, |slug, args| {
            let slug = slug.to_string();
            async move { composio.read_cached(&slug, &args).await }
        })
        .await;
        // A new command during those reads has the panel now.
        if !is_current(run.ticket) {
            return None;
        }
    }
    run.emit(app, AgentEvent::ToolCall(event));
    await_confirmation(app, call.args).await
}

/// The gate event for a write: the call plus the card that previews it, drawn
/// offline from the arguments and what the run's reads already named.
fn gate_call(
    name: String,
    toolkit: String,
    args: &serde_json::Value,
    schema: Option<&serde_json::Value>,
    ctx: &aloud_agent::preview::Context,
) -> ToolCall {
    ToolCall {
        preview: Some(Box::new(aloud_agent::preview::render(
            &name, args, schema, ctx,
        ))),
        app: toolkit,
        name,
        args: display_args(args),
        requires_confirmation: true,
    }
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
            preview: None,
        }),
        aloud_agent::LoopEvent::ToolCallResult {
            name,
            success,
            show,
        } => AgentEvent::ToolResult(ToolResult {
            success,
            message: if success {
                format!("{name} done")
            } else {
                format!("{name} failed")
            },
            show: show_of(show),
        }),
        aloud_agent::LoopEvent::RateLimited { seconds } => {
            AgentEvent::Status(format!("Rate limited — retrying in {seconds}s…"))
        }
        // The shimmering status line: replaced by each new step, cleared by the result.
        aloud_agent::LoopEvent::Progress { text, .. } => AgentEvent::Status(text),
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

/// Brings back what a finished step made — the note, the saved file, the
/// opened folder — from its row's Show button.
#[tauri::command]
#[specta::specta]
pub async fn show_result(app_name: String, target: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || aloud_native::show(&app_name, &target))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Lets the panel dismiss itself on Escape or after showing a result.
#[tauri::command]
#[specta::specta]
pub fn close_agent(app: AppHandle) {
    // Abandoning a pending confirmation reads as a rejection; the write never
    // fires. A dropped question sender resolves as unanswered, so a closed
    // panel never leaves the loop waiting out its full timeout.
    clear_pending(&app);
    hide(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A run is stopped only *between* tools, so a superseded one keeps
    /// executing and then emits — into the panel the next command already
    /// reset. That is how a new run showed the previous run's steps. Only the
    /// latest ticket may write to the panel.
    #[test]
    fn only_the_newest_run_owns_the_panel() {
        let first = claim_run();
        assert!(is_current(first));

        // The user speaks again while the first run is still mid-tool.
        let second = claim_run();
        assert!(is_current(second));
        assert!(
            !is_current(first),
            "the superseded run must not still own the panel"
        );

        // …and it stops at its next tool, even though its caller never
        // cancelled it.
        assert!(cancelled_or_superseded(first, &|| false));
        assert!(!cancelled_or_superseded(second, &|| false));
        assert!(cancelled_or_superseded(second, &|| true));

        // Tickets are strictly increasing, so a late run can never look current
        // again by coincidence.
        assert!(second > first);
        assert!(claim_run() > second);
    }

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

    /// A gate without its preview is the blank approval card this exists to
    /// end: the panel can only draw what the event carries, and it reads the
    /// card's fields by their camelCase names.
    #[test]
    fn a_gate_event_carries_its_preview() {
        let schema = json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "title": "Channel" },
                "markdown_text": { "type": "string" },
            }
        });
        let args = json!({ "channel": "general", "markdown_text": "Standup moves to 10:30" });
        let names = aloud_agent::preview::Names::default();
        let ctx = aloud_agent::preview::Context {
            said: "",
            names: &names,
        };
        let call = gate_call(
            "SLACK_CHAT_POST_MESSAGE".into(),
            "slack".into(),
            &args,
            Some(&schema),
            &ctx,
        );
        assert!(call.requires_confirmation);
        let sent = serde_json::to_value(&call).expect("serialises");
        assert_eq!(sent["preview"]["title"], "Send message to general");
        assert_eq!(sent["preview"]["fields"][0]["label"], "Channel");
        assert_eq!(
            sent["preview"]["fields"][1]["value"],
            "Standup moves to 10:30"
        );

        // The fast path has no model and so no tool list; its schema comes from
        // the native catalogue instead.
        let schema = aloud_native::params("ALOUD_QUIT_APP").expect("a native write");
        let call = gate_call(
            "ALOUD_QUIT_APP".into(),
            "macOS".into(),
            &json!({ "name": "Safari" }),
            Some(&schema),
            &ctx,
        );
        let preview = call.preview.expect("a preview");
        // Drawn by its hand mapping now, which names what it quits.
        assert_eq!(preview.title, "Quit Safari");
        assert_eq!(preview.fields[0].value, "Safari");
    }

    #[test]
    fn a_field_the_tool_never_had_arrives_as_a_string() {
        let edits = [("subject".to_string(), "Hello".to_string())]
            .into_iter()
            .collect();
        assert_eq!(apply_edits(&json!({}), &edits)["subject"], "Hello");
    }
}
