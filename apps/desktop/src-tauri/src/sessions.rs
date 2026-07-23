//! Wiring the agent bridge to the notch window.
//!
//! Orchestration only — the model, the discovery and the installer all live in
//! `aloud-sessions`, and the wire format in `aloud-bridge`. This file starts
//! the socket server, feeds both the socket and the discovery poll into the
//! one `SessionStore`, and emits the result.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use aloud_bridge::{Event as HookEvent, Reply};
use aloud_sessions::{install, SessionStore};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// One agent session, flattened for the webview.
///
/// `tool_input` is deliberately absent: specta will not export
/// `serde_json::Value`, and the running tool's arguments are arbitrary JSON.
/// The one line worth showing is already flattened to `detail` by the scan —
/// the same trick `agent.rs`'s `display_args()` uses.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    /// The agent's own session id where hooks are installed, else `pid:<n>`.
    /// What the UI keys a tab on, so it has to be stable across polls.
    pub id: String,
    pub agent: String,
    /// What to call the row: the model's title, the prompt, or the folder.
    pub title: String,
    /// The user's latest prompt, verbatim — what a "You:" line shows. The
    /// title is the model's and can be a session-old summary.
    pub prompt: Option<String>,
    pub cwd: Option<String>,
    /// The tool running right now, already flattened to one line.
    pub tool: Option<String>,
    /// The model, vendor prefix stripped: `opus-4-8`, `gpt-5.6-sol`.
    pub model: Option<String>,
    /// The git branch, for the row head: `aloud-tauri @ feat/agent-notch`.
    pub branch: Option<String>,
    /// ISO-8601 of last activity, for the row's `· now` / `· 10m` tag. The tag
    /// is rendered in the webview so it updates without a new event.
    pub last_activity: Option<String>,
    /// Subagents spawned under this session.
    pub subagents: Vec<AgentSubagent>,
    pub busy: bool,
    /// A recent one-shot stage — `"denied"` or `"session_ended"` — the webview
    /// shows over the tool-derived stage while it lasts, else `None`.
    pub transient: Option<String>,
}

/// A subagent under a session, for the `▸ N subagents` disclosure.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSubagent {
    pub label: String,
    pub model: Option<String>,
    pub tool: Option<String>,
    pub busy: bool,
}

/// The live session list, pushed whenever it actually changes.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "agent-sessions")]
pub struct AgentSessions(pub Vec<AgentSession>);

/// One request awaiting an Approve / Deny. The notch opens itself on this and
/// draws a card. Keyed by **session id** — the `PermissionRequest` payload
/// carries no `tool_use_id`, and the agent blocks its turn on the request, so
/// one card per session is the protocol's own invariant. Several *sessions*
/// can ask at once, so the panel still renders a queue.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "agent-permission")]
#[serde(rename_all = "camelCase")]
pub struct AgentPermission {
    /// The session id — how a decision is routed back to the waiting hook,
    /// and what ties the card to its row.
    pub id: String,
    /// Same as `id`; kept so the card→row linkage reads by name in the UI.
    pub session_id: Option<String>,
    pub agent: String,
    pub tool: String,
    /// The tool's arguments, flattened to strings the same way sessions are —
    /// specta will not export `serde_json::Value`.
    pub input: std::collections::BTreeMap<String, String>,
    pub cwd: Option<String>,
    /// Non-empty exactly when this is an `AskUserQuestion`: the card renders
    /// the model's own questions and options instead of Approve / Deny.
    pub questions: Vec<AgentQuestion>,
    /// Whether this arrival should raise the notch — the "open notch for
    /// permissions" setting, decided here so the webview and the window
    /// cannot disagree. Off: the card queues quietly behind the hover.
    pub open: bool,
}

/// One question on a card, mirroring the CLI's own prompt.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentQuestion {
    pub question: String,
    /// The short chip above the question — "Auth method", "Library".
    pub header: String,
    pub multi_select: bool,
    pub options: Vec<AgentQuestionOption>,
}

/// One choice under a question.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentQuestionOption {
    pub label: String,
    pub description: String,
}

/// What the user decided on a card, from the webview. Mirrors the terminal's
/// own three options plus the question path.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ApprovalReply {
    /// "Yes" — this call only.
    AllowOnce,
    /// "Yes, don't ask again for <tool> this session."
    AllowAlways,
    /// "No, and tell Claude what to do differently" — the text reaches the
    /// model as the deny message.
    Deny { message: Option<String> },
    /// The answers to an `AskUserQuestion`, keyed by each question's text;
    /// multi-select answers pre-joined with `", "`.
    Answer {
        answers: std::collections::BTreeMap<String, String>,
    },
}

/// A request has been answered or the agent gave up on it; the card comes down.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "agent-permission-resolved")]
pub struct AgentPermissionResolved {
    pub id: String,
}

/// A pending approval: which parked thread owns it, where to send the user's
/// answer, and enough of the request to build that answer — "always allow"
/// needs the tool name, a question's answer needs the original input back.
/// The token tells a waking thread whether the entry is still its own — a
/// newer request for the same session replaces the old entry, and the old
/// thread must not tear the new card down on its way out.
struct Pending {
    token: u64,
    reply: std::sync::mpsc::Sender<Reply>,
    tool: Option<String>,
    input: Option<serde_json::Value>,
}

/// Everything the notch needs to know, held across the poll and the socket.
#[derive(Default)]
pub struct Agents {
    /// The one table. Hook events and the discovery poll both feed it, and
    /// every emit renders it whole — a partial emit built from one source was
    /// how the session list used to blink.
    store: Mutex<SessionStore>,
    /// What was last emitted, so an unchanged poll sends nothing: a 3s no-op
    /// re-render restarts the mascot's CSS animation mid-stride.
    last: Mutex<Vec<AgentSession>>,
    /// Approvals waiting on a human, keyed by session id. The socket thread
    /// parks on the receiver; `resolve_permission` sends the answer. A std
    /// channel, not tokio's: the socket handler is a blocking thread, and
    /// blocking it is the whole mechanism — the agent is waiting on this
    /// hook's stdout.
    pending: Mutex<HashMap<String, Pending>>,
}

/// How often un-hooked sessions are re-discovered. Matches what the example
/// binary polls at, and the grace window is counted in polls.
const POLL: Duration = Duration::from_secs(3);

/// How often a parked approval checks whether its hook is still alive. The
/// wait is human-scale, so the probe only has to be faster than a person
/// noticing a stuck card.
const PENDING_PROBE: Duration = Duration::from_secs(5);

/// Starts the socket server and the discovery poll.
pub fn start(app: &AppHandle) {
    app.manage(Arc::new(Agents::default()));
    refresh_hook_binary(app);
    serve(app.clone());
    poll(app.clone());
}

/// Keeps the installed sidecar current with the app it shipped in.
///
/// The hook entries in `settings.json` point at a copy outside the bundle, so
/// a protocol change in `aloud-bridge` does nothing until that copy is
/// replaced — the stale sidecar deserializes the new reply, silently drops
/// the fields it never heard of, and the symptom is an answer that vanishes:
/// card down, agent never told. Refreshing on every launch closes that class.
///
/// Only refreshes, never installs: a missing target means hooks were never
/// set up, and that stays the user's explicit click.
fn refresh_hook_binary(app: &AppHandle) {
    let Ok(target) = hook_binary() else { return };
    if !target.exists() {
        return;
    }
    let Ok(source) = app
        .path()
        .resolve("aloud-hook", tauri::path::BaseDirectory::Resource)
        .or_else(|_| std::env::current_exe().map(|exe| exe.with_file_name("aloud-hook")))
    else {
        return;
    };
    if !source.exists() {
        return;
    }
    // Bytes, not mtimes: a rebuild that changes nothing should not churn the
    // file, and mtimes lie across copies.
    let same = matches!(
        (std::fs::read(&source), std::fs::read(&target)),
        (Ok(a), Ok(b)) if a == b
    );
    if !same {
        if let Err(e) = replace_binary(&source, &target) {
            tracing::warn!(%e, "could not refresh hook sidecar");
        } else {
            tracing::info!(path = %target.display(), "hook sidecar refreshed");
        }
    }
}

/// Replaces `target` with `source` via a sibling temp file and rename.
///
/// Never an in-place copy: a parked hook process is executing the old file,
/// and truncating an inode being executed kills it. The rename swaps the
/// directory entry; running instances keep their inode until they exit.
fn replace_binary(source: &std::path::Path, target: &std::path::Path) -> anyhow::Result<()> {
    let temp = target.with_extension("new");
    std::fs::copy(source, &temp)?;
    std::fs::rename(&temp, target)?;
    Ok(())
}

/// The discovery poll. The store merges it with whatever the hooks have said.
fn poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            // `ps` and `lsof` are blocking subprocesses; they have no business
            // on a runtime worker.
            let rows = tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<Arc<Agents>>();
                let mut store = state.store.lock().ok()?;
                store.poll();
                Some(rows(&store))
            })
            .await;

            if let Ok(Some(rows)) = rows {
                emit(&app, rows);
            }
            tokio::time::sleep(POLL).await;
        }
    });
}

/// The store, flattened for the webview. Always the whole table — both event
/// paths and the poll emit through this, so neither can drop the other's rows.
fn rows(store: &SessionStore) -> Vec<AgentSession> {
    store
        .sessions()
        .into_iter()
        .map(|s| AgentSession {
            id: s
                .session_id
                .clone()
                .unwrap_or_else(|| format!("pid:{}", s.pid)),
            agent: s.agent.label().to_string(),
            title: s.label(),
            prompt: s.prompt.clone(),
            cwd: s.cwd.clone(),
            tool: s.running.as_ref().map(describe),
            model: s.model.clone(),
            branch: s.branch.clone(),
            last_activity: s.last_activity.clone(),
            subagents: s
                .subagents
                .iter()
                .map(|sub| AgentSubagent {
                    label: sub.label.clone(),
                    model: sub.model.clone(),
                    tool: sub.running.as_ref().map(describe),
                    busy: sub.busy,
                })
                .collect(),
            busy: s.busy,
            transient: s.transient.clone(),
        })
        .collect()
}

/// Emits only on real change — see [`Agents::last`].
fn emit(app: &AppHandle, rows: Vec<AgentSession>) {
    let state = app.state::<Arc<Agents>>();
    let Ok(mut last) = state.last.lock() else {
        return;
    };
    if *last == rows {
        return;
    }
    *last = rows.clone();
    if let Err(e) = AgentSessions(rows).emit(app) {
        tracing::warn!(%e, "could not send agent sessions");
    }
}

/// Parks the socket thread until the user answers, then hands back their reply.
///
/// The agent is blocked on this hook's stdout for the whole wait, which is what
/// holds the request open. Three ways out: the user answers, a later hook event
/// shows the session moved on (answered in the terminal — the stale-clear in
/// `record`), or the hook process dies and the EOF probe notices. Every exit
/// but the first replies "no directive", and the agent proceeds on its own
/// rules rather than hanging.
#[cfg(unix)]
fn await_decision(
    app: &AppHandle,
    event: &HookEvent,
    stream: &std::os::unix::net::UnixStream,
) -> Reply {
    static TOKEN: AtomicU64 = AtomicU64::new(0);

    // Routed by session id: the PermissionRequest payload carries no
    // tool_use_id — requiring one here silently dropped every request, which
    // looked exactly like the event never firing. See AGENT_NOTCH.md.
    let Some(id) = event.session_id.clone() else {
        return Reply::default();
    };
    let token = TOKEN.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel::<Reply>();

    let state = app.state::<Arc<Agents>>();
    {
        let Ok(mut pending) = state.pending.lock() else {
            return Reply::default();
        };
        // A newer request replaces a stale one: dropping the old sender wakes
        // its thread, which answers its hook "no directive" and leaves the
        // card alone — same id, so the new request already replaced it there.
        pending.insert(
            id.clone(),
            Pending {
                token,
                reply: tx,
                tool: event.tool_name.clone(),
                input: event.tool_input.clone(),
            },
        );
    }

    // Whether this request raises the notch, per settings. The hook parks
    // either way — the toggle is about the surface, never the protocol.
    let open = app
        .try_state::<crate::AppState>()
        .and_then(|state| {
            state
                .settings
                .lock()
                .ok()
                .map(|s| s.agent_open_on_permission)
        })
        .unwrap_or(true);

    // The card. `tool_input` is arbitrary JSON; flatten it to strings the way
    // the sessions do, keeping the real Value out of the binding.
    let card = AgentPermission {
        id: id.clone(),
        session_id: Some(id.clone()),
        agent: match event.source {
            Some(aloud_bridge::Source::Codex) => "codex",
            _ => "claude",
        }
        .to_string(),
        tool: event.tool_name.clone().unwrap_or_default(),
        input: flatten_args(event.tool_input.as_ref()),
        cwd: event.cwd.clone(),
        questions: event
            .questions()
            .into_iter()
            .map(|q| AgentQuestion {
                question: q.question,
                header: q.header,
                multi_select: q.multi_select,
                options: q
                    .options
                    .into_iter()
                    .map(|o| AgentQuestionOption {
                        label: o.label,
                        description: o.description,
                    })
                    .collect(),
            })
            .collect(),
        open,
    };
    if let Err(e) = card.emit(app) {
        tracing::warn!(%e, "could not send permission card");
    }

    // A chime, so a blocked agent is noticed even when the notch is not being
    // watched — the audio counterpart of raising the notch. Gated on the same
    // sound settings as the dictation chimes; distinct tone (`Attention`) so it
    // never reads as "recording started". Fire-and-forget, like `play` itself.
    if let Some((enabled, volume)) = app.try_state::<crate::AppState>().and_then(|state| {
        state
            .settings
            .lock()
            .ok()
            .map(|s| (s.play_sounds, s.sound_volume))
    }) {
        crate::sound::play(crate::sound::Chime::Attention, enabled, volume);
    }

    // Bring the notch forward so the card is not missed — unless the user
    // asked it not to. `show_panel` drives NSWindow, which is
    // main-thread-only — and this runs on the socket's OS thread, so it has
    // to be marshalled across. (The trap CLAUDE.md names.)
    if open {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || crate::overlay::show_panel(&handle));
    }

    // Park, probing the socket each lap: a thread blocked on a channel cannot
    // see EOF, and a hook that died waiting must not strand its card. The
    // hook's own 24h read timeout (aloud-bridge) is the outer ceiling.
    let reply = loop {
        match rx.recv_timeout(PENDING_PROBE) {
            Ok(reply) => break reply,
            // Sender dropped: replaced by a newer request, cleared as stale,
            // or the app is shutting down. No directive in every case.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break Reply::default(),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if hook_gone(stream) {
                    break Reply::default();
                }
            }
        }
    };

    // Clean up only what is still ours. Entry gone → someone else concluded
    // it (user click, stale-clear); emitting resolved again is harmless and
    // covers the paths that could not. A *different* token means a newer
    // request owns the card now — leave both entry and card alone.
    let mut resolved = true;
    if let Ok(mut pending) = state.pending.lock() {
        match pending.get(&id) {
            Some(p) if p.token == token => {
                pending.remove(&id);
            }
            Some(_) => resolved = false,
            None => {}
        }
    }
    if resolved {
        let _ = (AgentPermissionResolved { id }).emit(app);
    }
    reply
}

/// Whether the hook process on the other end has gone away.
///
/// A zero-byte read is EOF — the peer closed. The socket is briefly flipped
/// to non-blocking so the probe cannot itself park; `WouldBlock` is the
/// healthy answer (connected, nothing to read — the hook sends one line and
/// then only listens).
#[cfg(unix)]
fn hook_gone(stream: &std::os::unix::net::UnixStream) -> bool {
    use std::io::Read;
    if stream.set_nonblocking(true).is_err() {
        return true;
    }
    let mut probe = [0u8; 1];
    let gone = match (&mut &*stream).read(&mut probe) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(_) => true,
    };
    // Back to blocking: the eventual directive write must not WouldBlock.
    let _ = stream.set_nonblocking(false);
    gone
}

/// Flattens `tool_input` to display strings — specta will not export a
/// `serde_json::Value`, and the card only needs it readable, not round-tripped.
fn flatten_args(input: Option<&serde_json::Value>) -> std::collections::BTreeMap<String, String> {
    input
        .and_then(|v| v.as_object())
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

/// Resolve a pending request, by session id. The socket thread parked on it
/// wakes with this reply and writes the directive to the agent.
#[tauri::command]
#[specta::specta]
pub fn resolve_permission(app: AppHandle, id: String, reply: ApprovalReply) {
    let state = app.state::<Arc<Agents>>();
    let entry = state
        .pending
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&id));
    if let Some(entry) = entry {
        let denied = matches!(reply, ApprovalReply::Deny { .. });
        let reply = match reply {
            ApprovalReply::AllowOnce => Reply::allow(),
            // No tool name to write a rule for degrades to a plain allow —
            // wrong to refuse the click over a display detail.
            ApprovalReply::AllowAlways => match entry.tool.as_deref() {
                Some(tool) if !tool.is_empty() => Reply::allow_always(tool),
                _ => Reply::allow(),
            },
            ApprovalReply::Deny { message } => Reply::deny(
                message
                    .filter(|m| !m.trim().is_empty())
                    .unwrap_or_else(|| "Denied in Aloud".to_string()),
            ),
            ApprovalReply::Answer { answers } => Reply::answer(entry.input.as_ref(), &answers),
        };
        // The receiver is gone only if the request already concluded; nothing
        // to do then, the agent has moved on.
        let _ = entry.reply.send(reply);

        // A denial is a one-shot the row should flash — mark it and re-emit so
        // the rejected cat shows now, not on the next 3s poll. `id` is the
        // session id (pending is keyed by it), which is what the store wants.
        if denied {
            if let Ok(mut store) = state.store.lock() {
                store.mark_denied(&id, std::time::SystemTime::now());
                let rows = rows(&store);
                drop(store);
                emit(&app, rows);
            }
        }
    }
}

/// One line describing a running tool, for a narrow row.
fn describe(tool: &aloud_sessions::Tool) -> String {
    match &tool.detail {
        Some(detail) => format!("{}: {detail}", tool.name),
        None => tool.name.clone(),
    }
}

/// The socket server. One connection per hook firing, one line each way.
fn serve(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(path) = aloud_bridge::socket::socket_path() else {
            tracing::warn!("no bridge socket path; agent hooks disabled");
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // A stale socket from a crash would make `bind` fail with "address in
        // use" forever; the path is ours, so removing it first is safe.
        let _ = std::fs::remove_file(&path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let listener = match std::os::unix::net::UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(e) => {
                    tracing::warn!(%e, "could not bind agent bridge");
                    return;
                }
            };
            // Anything that can write here can approve a tool call.
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            tracing::info!(path = %path.display(), "agent bridge listening");

            for stream in listener.incoming().flatten() {
                let app = app.clone();
                // One thread per event rather than a pool: a PermissionRequest
                // parks for up to a day, and it must not hold up the status
                // events arriving behind it.
                std::thread::spawn(move || handle(&app, stream));
            }
        }
    });
}

#[cfg(unix)]
fn handle(app: &AppHandle, stream: std::os::unix::net::UnixStream) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let Ok(event) = serde_json::from_str::<HookEvent>(line.trim()) else {
        return;
    };

    record(app, &event);

    // Only a PermissionRequest parks for a decision — it fires exactly when
    // the agent would have asked the user, so permission modes and allowlists
    // are already respected. Everything else is fire-and-forget status.
    let reply = if event.is_permission_request() {
        await_decision(app, &event, &stream)
    } else {
        Reply::default()
    };

    if let Ok(mut text) = serde_json::to_string(&reply) {
        text.push('\n');
        let mut writer = &stream;
        let _ = writer.write_all(text.as_bytes());
        let _ = writer.flush();
    }
}

/// Hook events for a session that has a pending approval and is visibly past
/// it — the user answered in the terminal, or the turn ended some other way.
/// The stale card must come down on its own.
const RESOLVING_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SessionEnd",
];

/// Folds one hook event into the store and emits the merged list.
fn record(app: &AppHandle, event: &HookEvent) {
    let state = app.state::<Arc<Agents>>();

    // Clear a stale approval: a lifecycle event means the session moved past
    // the request without us. Dropping the sender wakes the parked thread,
    // which answers its hook "no directive" and takes the card down.
    let name = event.hook_event_name.as_deref().unwrap_or_default();
    if RESOLVING_EVENTS.contains(&name) {
        if let (Some(id), Ok(mut pending)) = (event.session_id.as_ref(), state.pending.lock()) {
            pending.remove(id);
        }
    }

    let Ok(mut store) = state.store.lock() else {
        return;
    };
    store.apply(event, SystemTime::now());
    let rows = rows(&store);
    drop(store);
    // The poll would pick this up within 3s anyway; emitting now is what makes
    // a tool call show up the moment it starts rather than a poll later.
    emit(app, rows);
}

/// Where the sidecar is installed to, outside the `.app` bundle.
///
/// A path *inside* the bundle breaks the moment the app is moved or updated
/// while `settings.json` keeps pointing at it, so the binary is copied out and
/// the config points at the copy.
fn hook_binary() -> anyhow::Result<std::path::PathBuf> {
    let dir = aloud_notes::data_dir()?.join("bin");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("aloud-hook"))
}

/// The Claude-format settings file.
fn claude_settings() -> anyhow::Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    Ok(home.join(".claude/settings.json"))
}

/// What the settings pane needs to draw: whether ours are in, and who owns
/// approvals if anyone.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HookStatus {
    pub installed: Vec<String>,
    /// The command owning `PermissionRequest`, when it is not ours.
    pub conflict: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn agent_hook_status() -> Result<HookStatus, String> {
    let path = claude_settings().map_err(|e| e.to_string())?;
    let settings = read_settings(&path).map_err(|e| e.to_string())?;
    Ok(HookStatus {
        installed: install::installed_events(&settings),
        conflict: match install::permission_conflict(&settings) {
            install::Conflict::Foreign(command) => Some(command),
            install::Conflict::None => None,
        },
    })
}

fn read_settings(path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => Ok(serde_json::from_str(&text)?),
        // No file yet is the first-run case, not an error.
        _ => Ok(serde_json::json!({})),
    }
}

/// Installs our hooks, copying the sidecar out of the bundle first.
///
/// Approvals ride along unless another tool owns `PermissionRequest` — the
/// event only fires when the agent would have asked the user anyway, so
/// taking it is not putting Aloud in front of calls the user already allowed.
/// A foreign owner downgrades this to status-only; taking over is a separate,
/// deliberate act (`install::take_over`) that the settings pane will own.
#[tauri::command]
#[specta::specta]
pub fn install_agent_hooks(app: AppHandle) -> Result<HookStatus, String> {
    let target = hook_binary().map_err(|e| e.to_string())?;
    // The sidecar ships beside the app binary; copy it out so the recorded path
    // survives the bundle moving.
    let source = app
        .path()
        .resolve("aloud-hook", tauri::path::BaseDirectory::Resource)
        .or_else(|_| std::env::current_exe().map(|exe| exe.with_file_name("aloud-hook")))
        .map_err(|e| e.to_string())?;
    if source.exists() {
        replace_binary(&source, &target).map_err(|e| e.to_string())?;
    } else if !target.exists() {
        return Err(format!("hook binary not found at {}", source.display()));
    }

    let path = claude_settings().map_err(|e| e.to_string())?;
    let settings = read_settings(&path).map_err(|e| e.to_string())?;
    let command = format!("'{}' --source claude", target.display());
    let approvals = matches!(
        install::permission_conflict(&settings),
        install::Conflict::None
    );
    let updated = install::install(&settings, &command, approvals);
    write_settings(&path, &updated).map_err(|e| e.to_string())?;
    agent_hook_status()
}

#[tauri::command]
#[specta::specta]
pub fn uninstall_agent_hooks() -> Result<HookStatus, String> {
    let path = claude_settings().map_err(|e| e.to_string())?;
    let settings = read_settings(&path).map_err(|e| e.to_string())?;
    write_settings(&path, &install::uninstall(&settings)).map_err(|e| e.to_string())?;
    agent_hook_status()
}

/// Which CLI a new coding-agent tab opens. Its own type rather than a string so
/// the binding is a union, not `string`, and an unknown agent is a type error at
/// the call site instead of a silent fall-through in Rust.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CodingAgent {
    Claude,
    Codex,
}

impl From<CodingAgent> for aloud_agent_spawn::Agent {
    fn from(agent: CodingAgent) -> Self {
        match agent {
            CodingAgent::Claude => aloud_agent_spawn::Agent::Claude,
            CodingAgent::Codex => aloud_agent_spawn::Agent::Codex,
        }
    }
}

/// Cuts a worktree off `origin/main` in `repo` and opens `agent` on `task` in a
/// Terminal tab there. Returns the new branch name so the panel can show it
/// without waiting on the discovery poll that will surface the session itself.
///
/// The spawned process is a normal interactive session: the existing `ps`/`lsof`
/// discovery finds it, and the globally-installed hooks fire for it, so no new
/// tracking path is needed here — only the starting of it.
#[tauri::command]
#[specta::specta]
pub fn spawn_coding_agent(repo: String, task: String, agent: CodingAgent) -> Result<String, String> {
    aloud_agent_spawn::start_coding_agent(std::path::Path::new(&repo), &task, agent.into())
        .map_err(|e| e.to_string())
}

/// Writes the config, pretty-printed, via a temp file in the same directory.
///
/// A partial write here is another tool's config destroyed, so the rename is
/// what makes it atomic — and the temp file must share the filesystem for the
/// rename to be one.
fn write_settings(path: &std::path::Path, value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("aloud-tmp");
    std::fs::write(&temp, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    std::fs::rename(&temp, path)?;
    Ok(())
}
