//! Wiring the agent bridge to the notch window.
//!
//! Orchestration only — the model, the discovery and the installer all live in
//! `aloud-sessions`, and the wire format in `aloud-bridge`. This file starts
//! the socket server, merges what it hears with the discovery poll, and emits
//! the result.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    pub cwd: Option<String>,
    /// The tool running right now, already flattened to one line.
    pub tool: Option<String>,
    /// The model, vendor prefix stripped: `opus-4-8`, `gpt-5.6-sol`.
    pub model: Option<String>,
    /// ISO-8601 of last activity, for the row's `· now` / `· 10m` tag. The tag
    /// is rendered in the webview so it updates without a new event.
    pub last_activity: Option<String>,
    /// Subagents spawned under this session.
    pub subagents: Vec<AgentSubagent>,
    pub busy: bool,
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

/// One tool call awaiting an Approve / Deny. The notch opens itself on this and
/// draws a card; several can be in flight at once (a Codex swarm asks in
/// parallel), so the panel renders them as a queue keyed by `id`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "agent-permission")]
pub struct AgentPermission {
    /// `tool_use_id` — how a decision is routed back to the waiting hook.
    pub id: String,
    /// The agent's session id, so the card can be shown under the exact row
    /// that is asking — with several terminals open, this is what tells them
    /// apart.
    pub session_id: Option<String>,
    pub agent: String,
    pub tool: String,
    /// The tool's arguments, flattened to strings the same way sessions are —
    /// specta will not export `serde_json::Value`.
    pub input: std::collections::BTreeMap<String, String>,
    pub cwd: Option<String>,
}

/// A request has been answered or the agent gave up on it; the card comes down.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "agent-permission-resolved")]
pub struct AgentPermissionResolved {
    pub id: String,
}

/// Everything the notch needs to know, held across the poll and the socket.
#[derive(Default)]
pub struct Agents {
    store: Mutex<SessionStore>,
    /// What was last emitted, so an unchanged poll sends nothing: a 3s no-op
    /// re-render restarts the mascot's CSS animation mid-stride.
    last: Mutex<Vec<AgentSession>>,
    /// Sessions hooks have told us about, keyed by the agent's own session id.
    /// Hook data wins over discovery, because it is the agent speaking.
    hooked: Mutex<HashMap<String, AgentSession>>,
    /// Tool calls waiting on an Approve / Deny, keyed by `tool_use_id`. The
    /// socket thread parks on the receiver; `resolve_permission` sends the
    /// answer. A std channel, not tokio's: the socket handler is a blocking
    /// thread, and blocking it is the whole mechanism — the agent is waiting on
    /// this hook's stdout.
    pending: Mutex<HashMap<String, std::sync::mpsc::Sender<aloud_bridge::Reply>>>,
}

/// How often un-hooked sessions are re-discovered. Matches what the example
/// binary polls at, and the grace window is counted in polls.
const POLL: Duration = Duration::from_secs(3);

/// Starts the socket server and the discovery poll.
pub fn start(app: &AppHandle) {
    app.manage(Arc::new(Agents::default()));
    serve(app.clone());
    poll(app.clone());
}

/// The discovery poll, merged with whatever the hooks have said.
fn poll(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            // `ps` and `lsof` are blocking subprocesses; they have no business
            // on a runtime worker.
            let discovered = tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<Arc<Agents>>();
                let mut store = state.store.lock().ok()?;
                store.poll();
                Some(store.sessions())
            })
            .await;

            if let Ok(Some(sessions)) = discovered {
                let state = app.state::<Arc<Agents>>();
                let mut rows: Vec<AgentSession> = sessions
                    .into_iter()
                    .map(|s| AgentSession {
                        id: format!("pid:{}", s.pid),
                        agent: s.agent.label().to_string(),
                        title: s.label(),
                        cwd: s.cwd.clone(),
                        tool: s.running.as_ref().map(describe),
                        model: s.model.clone(),
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
                    })
                    .collect();

                // Hook data wins: it carries the agent's own session id, which
                // discovery cannot observe, so a hooked session replaces the
                // process-shaped guess rather than appearing twice beside it.
                if let Ok(hooked) = state.hooked.lock() {
                    let by_cwd: Vec<&AgentSession> = hooked.values().collect();
                    rows.retain(|row| !by_cwd.iter().any(|h| h.cwd.is_some() && h.cwd == row.cwd));
                    rows.extend(by_cwd.into_iter().cloned());
                }
                rows.sort_by(|a, b| (!a.busy, &a.id).cmp(&(!b.busy, &b.id)));
                emit(&app, rows);
            }

            tokio::time::sleep(POLL).await;
        }
    });
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
/// holds the tool call open. If anything goes wrong — no id to route on, the app
/// shutting the channel — it falls through to no directive, and the agent
/// proceeds on its own rules rather than hanging.
#[cfg(unix)]
fn await_decision(app: &AppHandle, event: &HookEvent) -> Reply {
    let Some(id) = event.tool_use_id.clone() else {
        return Reply::default();
    };
    let (tx, rx) = std::sync::mpsc::channel::<Reply>();

    let state = app.state::<Arc<Agents>>();
    {
        let Ok(mut pending) = state.pending.lock() else {
            return Reply::default();
        };
        pending.insert(id.clone(), tx);
    }

    // The card. `tool_input` is arbitrary JSON; flatten it to strings the way
    // the sessions do, keeping the real Value out of the binding.
    let card = AgentPermission {
        id: id.clone(),
        // Matches the hooked row's id (see `record`), so the webview can show
        // the card under the exact agent that is asking.
        session_id: event.session_id.clone(),
        agent: match event.source {
            Some(aloud_bridge::Source::Codex) => "codex",
            _ => "claude",
        }
        .to_string(),
        tool: event.tool_name.clone().unwrap_or_default(),
        input: flatten_args(event.tool_input.as_ref()),
        cwd: event.cwd.clone(),
    };
    if let Err(e) = card.emit(app) {
        tracing::warn!(%e, "could not send permission card");
    }

    // Bring the notch forward so the card is not missed. `show_panel` drives
    // NSWindow, which is main-thread-only — and this runs on the socket's OS
    // thread, so it has to be marshalled across. (The trap CLAUDE.md names.)
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::overlay::show_panel(&handle));

    // No inner timeout: the hook's own 24h read timeout (aloud-bridge) is the
    // ceiling, and it closing the socket is what unblocks a request the user
    // never answered. `recv` returning `Err` (the sender dropped on shutdown)
    // takes the same fall-through as everything else.
    let reply = rx.recv().unwrap_or_default();

    // Whether answered or abandoned, the card comes down and the slot clears.
    if let Ok(mut pending) = state.pending.lock() {
        pending.remove(&id);
    }
    let _ = (AgentPermissionResolved { id }).emit(app);
    reply
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

/// Approve or deny a pending request. The socket thread parked on it wakes with
/// this reply and writes the directive to the agent.
#[tauri::command]
#[specta::specta]
pub fn resolve_permission(app: AppHandle, id: String, allow: bool) {
    let state = app.state::<Arc<Agents>>();
    let sender = state
        .pending
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&id));
    if let Some(sender) = sender {
        let reply = if allow {
            Reply::allow()
        } else {
            Reply::deny("Denied in Aloud")
        };
        // The receiver is gone only if the request already timed out; nothing
        // to do then, the agent has moved on.
        let _ = sender.send(reply);
    }
}

/// Now, in the same ISO-8601 UTC shape the transcripts use, so the webview's
/// relative-time formatter takes hook rows and discovery rows the same way.
fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
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

    // A gate-worthy call parks here until the user answers; everything else is
    // fire-and-forget and gets no directive. On current Claude builds the gate
    // rides in on `PreToolUse` for a write/bash tool — `PermissionRequest` is
    // silent — so `wants_gate` covers both.
    let reply = if event.wants_gate() {
        await_decision(app, &event)
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

/// Folds one hook event into the session list.
fn record(app: &AppHandle, event: &HookEvent) {
    let Some(id) = event.session_id.clone() else {
        return;
    };
    let state = app.state::<Arc<Agents>>();
    let Ok(mut hooked) = state.hooked.lock() else {
        return;
    };

    let name = event.hook_event_name.as_deref().unwrap_or_default();
    // The session is over: drop it rather than leaving a row that never clears.
    if matches!(name, "SessionEnd") {
        hooked.remove(&id);
    } else {
        let row = hooked.entry(id.clone()).or_insert_with(|| AgentSession {
            id,
            agent: match event.source {
                Some(aloud_bridge::Source::Codex) => "codex",
                _ => "claude",
            }
            .to_string(),
            title: String::new(),
            cwd: None,
            tool: None,
            model: None,
            last_activity: None,
            // Subagents come from the transcript, not the hook payload, so the
            // discovery poll fills these in — the hook only knows the parent.
            subagents: Vec::new(),
            busy: false,
        });
        if event.cwd.is_some() {
            row.cwd = event.cwd.clone();
        }
        if event.model.is_some() {
            row.model = event
                .model
                .as_deref()
                .map(|m| m.strip_prefix("claude-").unwrap_or(m).to_string());
        }
        // Every hook fires with a wall-clock now; ISO-stamp it so the row's
        // relative tag ticks the same way discovery's does.
        row.last_activity = Some(now_iso());
        if let Some(prompt) = &event.prompt {
            row.title = prompt.clone();
        }
        if row.title.is_empty() {
            row.title = row
                .cwd
                .as_deref()
                .and_then(|c| std::path::Path::new(c).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| row.agent.clone());
        }
        // The turn's shape: a tool starting means busy, `Stop` means the turn
        // ended. `PostToolUse` clears the tool without clearing busy — the
        // agent is thinking about what it just read.
        match name {
            "PreToolUse" => {
                row.busy = true;
                row.tool = event.tool_name.clone();
            }
            "PostToolUse" => row.tool = None,
            "UserPromptSubmit" | "SessionStart" | "SubagentStart" => row.busy = true,
            "Stop" => {
                row.busy = false;
                row.tool = None;
            }
            _ => {}
        }
    }

    let rows: Vec<AgentSession> = hooked.values().cloned().collect();
    drop(hooked);
    // The poll would pick this up within 3s anyway; emitting now is what makes
    // a tool call show up the moment it starts rather than a poll later.
    let mut merged = rows;
    merged.sort_by(|a, b| (!a.busy, &a.id).cmp(&(!b.busy, &b.id)));
    emit(app, merged);
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
/// Status events only. Approvals stay off until step 4 can answer them — and
/// on this machine another tool owns that event anyway.
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
        std::fs::copy(&source, &target).map_err(|e| e.to_string())?;
    } else if !target.exists() {
        return Err(format!("hook binary not found at {}", source.display()));
    }

    let path = claude_settings().map_err(|e| e.to_string())?;
    let settings = read_settings(&path).map_err(|e| e.to_string())?;
    let command = format!("'{}' --source claude", target.display());
    let updated = install::install(&settings, &command, false);
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
