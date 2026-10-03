//! The engine child process and the task handle.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot, Mutex};

/// A task longer than this is stopped: the agent's run must end, and a stuck page never does.
const TASK_TIMEOUT: Duration = Duration::from_secs(180);
/// How long a single RPC reply may take (starting a task launches a browser the first time).
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Replies awaited by request id.
type Waiting = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct Engine {
    dir: PathBuf,
    stdin: Mutex<ChildStdin>,
    waiting: Waiting,
    events: broadcast::Sender<Value>,
    next: AtomicU64,
    /// Cleared by the reader when the engine's stdout closes (it exited or crashed).
    alive: Arc<AtomicBool>,
    _child: Child,
}

static ENGINE: Mutex<Option<Arc<Engine>>> = Mutex::const_new(None);

/// Apps launched from Finder get a minimal PATH, so an nvm-installed node isn't on it: take
/// `VOICE_MAC_NODE`, else ask a login shell (which loads nvm), else the Homebrew locations.
fn node() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VOICE_MAC_NODE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    let out = std::process::Command::new("/bin/zsh")
        .args(["-lic", "command -v node"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let found = String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()
        .map(|l| PathBuf::from(l.trim()));
    found.filter(|p| p.is_file()).or_else(|| {
        ["/opt/homebrew/bin/node", "/usr/local/bin/node"]
            .map(PathBuf::from)
            .into_iter()
            .find(|p| p.is_file())
    })
}

async fn engine(dir: &Path) -> Result<Arc<Engine>, String> {
    let mut slot = ENGINE.lock().await;
    if let Some(e) = slot.as_ref() {
        // A dead child (crash, killed) or a changed folder gets a fresh engine.
        if e.dir == dir && e.alive.load(Ordering::Relaxed) {
            return Ok(e.clone());
        }
    }
    let node = node().ok_or("Node.js wasn't found; install it or set VOICE_MAC_NODE")?;
    // API keys: the .env staged beside the engine in the bundle, else the repo root's in development.
    let env_file = [dir.join("../.env"), dir.join("../../.env")]
        .into_iter()
        .find(|p| p.is_file());
    let mut args: Vec<String> = env_file
        .iter()
        .map(|p| format!("--env-file={}", p.display()))
        .collect();
    args.push("rpc.ts".to_string());
    // A signed bundle is read-only: the browser profile and the log live in Application Support.
    let data = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Library/Application Support/Aloud/engine");
    let mut command = Command::new(&node);
    // The accessibility helper: bundled beside the engine, or built next to its source in development.
    if let Some(axd) = [dir.join("../axd"), dir.join("axd/axd")]
        .into_iter()
        .find(|p| p.is_file())
    {
        command.env("VOICE_MAC_AXD", axd);
    }
    let mut child = command
        .args(&args)
        .current_dir(dir)
        .env("VOICE_MAC_DATA", data)
        .env(
            "PATH",
            format!(
                "{}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
                node.parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("couldn't start the Voice Mac engine: {e}"))?;
    let stdin = child.stdin.take().ok_or("engine stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("engine stdout unavailable")?;
    let (events, _) = broadcast::channel(256);
    let waiting: Waiting = Arc::default();
    let (ready_tx, ready_rx) = oneshot::channel();
    let alive = Arc::new(AtomicBool::new(true));
    {
        let events = events.clone();
        let waiting = waiting.clone();
        let alive = alive.clone();
        tokio::spawn(async move {
            let mut ready = Some(ready_tx);
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if msg["event"] == "ready" {
                    if let Some(tx) = ready.take() {
                        let _ = tx.send(());
                    }
                } else if let Some(id) = msg["id"].as_u64() {
                    if let Some(tx) = waiting
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&id)
                    {
                        let reply = match msg.get("error") {
                            Some(err) => Err(err["message"]
                                .as_str()
                                .unwrap_or("engine error")
                                .to_string()),
                            None => Ok(msg["result"].clone()),
                        };
                        let _ = tx.send(reply);
                    }
                } else if msg.get("event").is_some() {
                    let _ = events.send(msg);
                }
            }
            // stdout closed: the engine is gone. Fail everyone still waiting.
            alive.store(false, Ordering::Relaxed);
            for (_, tx) in waiting.lock().unwrap_or_else(|p| p.into_inner()).drain() {
                let _ = tx.send(Err("the Voice Mac engine stopped".to_string()));
            }
        });
    }
    tokio::time::timeout(Duration::from_secs(30), ready_rx)
        .await
        .map_err(|_| "the Voice Mac engine didn't start within 30 s".to_string())?
        .map_err(|_| {
            "the Voice Mac engine exited while starting; check its .env and `npm install`"
                .to_string()
        })?;
    let e = Arc::new(Engine {
        dir: dir.to_path_buf(),
        stdin: Mutex::new(stdin),
        waiting,
        events,
        next: AtomicU64::new(1),
        alive,
        _child: child,
    });
    *slot = Some(e.clone());
    Ok(e)
}

impl Engine {
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.waiting
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let mut line = serde_json::to_vec(&json!({ "id": id, "method": method, "params": params }))
            .map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.stdin
            .lock()
            .await
            .write_all(&line)
            .await
            .map_err(|e| format!("the Voice Mac engine stopped: {e}"))?;
        tokio::time::timeout(CALL_TIMEOUT, rx)
            .await
            .map_err(|_| format!("the Voice Mac engine didn't answer {method}"))?
            .map_err(|_| "the Voice Mac engine stopped".to_string())?
    }
}

/// What a running task reports next.
#[derive(Debug)]
pub enum Update {
    /// One step happened, described for a person ("Clicked “Downloads”").
    Step(String),
    /// The task is paused before a risky action; answer with [`Task::approve`].
    NeedsApproval(String),
    /// The task ended. The JSON is the tool result for the model.
    Finished(Value),
}

/// A running engine task. Drive it with [`Task::next`] until [`Update::Finished`].
pub struct Task {
    engine: Arc<Engine>,
    id: String,
    events: broadcast::Receiver<Value>,
    started: tokio::time::Instant,
    asked: bool,
}

/// The fast path: the engine routes a spoken command with one Jev request and starts it when it is a
/// Mac or web task. `Ok(None)` hands it back to the caller (email, calendar, answers, unsure routing).
pub(crate) async fn command(
    dir: &Path,
    text: &str,
    front: Option<&str>,
) -> Result<Option<Task>, String> {
    let engine = engine(dir).await?;
    let events = engine.events.subscribe();
    let result = engine
        .call("command", json!({ "text": text, "front": front }))
        .await?;
    let Some(id) = result["id"].as_str() else {
        return Ok(None);
    };
    Ok(Some(Task {
        engine,
        id: id.to_string(),
        events,
        started: tokio::time::Instant::now(),
        asked: false,
    }))
}

pub(crate) async fn start_task(dir: &Path, method: &str, params: Value) -> Result<Task, String> {
    let engine = engine(dir).await?;
    // Subscribe before starting so the first step can't be missed.
    let events = engine.events.subscribe();
    let result = engine.call(method, params).await?;
    let id = result["id"]
        .as_str()
        .ok_or("the engine returned no task id")?
        .to_string();
    Ok(Task {
        engine,
        id,
        events,
        started: tokio::time::Instant::now(),
        asked: false,
    })
}

impl Task {
    /// Which tool this task belongs to: Mac task ids start with "m-".
    pub fn tool(&self) -> &'static str {
        if self.id.starts_with("m-") {
            crate::MAC_TASK
        } else {
            crate::BROWSER_TASK
        }
    }

    /// The next update. Returns within ~250 ms even when nothing happened (`None`), so the caller can
    /// check for cancellation between waits.
    pub async fn next(&mut self) -> Option<Update> {
        if self.started.elapsed() > TASK_TIMEOUT {
            let _ = self.stop().await;
            return Some(Update::Finished(
                json!({ "error": format!("the task took longer than {} s and was stopped", TASK_TIMEOUT.as_secs()) }),
            ));
        }
        let msg = match tokio::time::timeout(Duration::from_millis(250), self.events.recv()).await {
            Ok(Ok(msg)) => msg,
            Ok(Err(broadcast::error::RecvError::Lagged(_))) | Err(_) => return None,
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                return Some(Update::Finished(
                    json!({ "error": "the Voice Mac engine stopped mid-task" }),
                ))
            }
        };
        if msg["id"].as_str() != Some(self.id.as_str()) {
            return None;
        }
        let steps: Vec<String> = msg["steps"]
            .as_array()
            .map(|s| s.iter().map(describe).collect())
            .unwrap_or_default();
        let status = msg["status"].as_str().unwrap_or_default();
        if status == "waiting" && !self.asked {
            self.asked = true;
            let label = msg["pending"]["label"]
                .as_str()
                .unwrap_or("the next action");
            return Some(Update::NeedsApproval(format!("Allow this step: {label}?")));
        }
        if matches!(status, "done" | "stuck" | "stopped" | "error") {
            return Some(Update::Finished(self.summary(status, &msg).await));
        }
        steps.into_iter().last().map(Update::Step)
    }

    pub async fn approve(&mut self, ok: bool) -> Result<(), String> {
        self.asked = false;
        self.engine
            .call("task.approve", json!({ "id": self.id, "ok": ok }))
            .await
            .map(|_| ())
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        self.engine
            .call("task.stop", json!({ "id": self.id }))
            .await
            .map(|_| ())
    }

    /// The tool result: status, the answer the engine quoted from the page or app, and the steps.
    async fn summary(&self, status: &str, last: &Value) -> Value {
        let all = self
            .engine
            .call("task.progress", json!({ "id": self.id, "from": 0 }))
            .await
            .unwrap_or_else(|_| last.clone());
        let steps: Vec<String> = all["steps"]
            .as_array()
            .map(|s| s.iter().map(describe).collect())
            .unwrap_or_default();
        let mut out = json!({
            "successful": status == "done",
            "status": status,
            "steps": steps,
        });
        if let Some(answer) = all.get("answer").filter(|a| !a.is_null()) {
            out["answer"] = answer["lines"].clone();
            if let Some(url) = answer["url"].as_str() {
                out["url"] = json!(url);
            }
        }
        if let Some(err) = all["error"].as_str() {
            out["error"] = json!(err);
        } else if status != "done" {
            let why = steps
                .last()
                .cloned()
                .unwrap_or_else(|| "it couldn't continue".to_string());
            out["error"] = json!(format!("the task ended as {status}: {why}"));
        }
        out
    }
}

/// One engine step as a short sentence.
fn describe(step: &Value) -> String {
    let target = step["target"]["label"].as_str().unwrap_or_default();
    let text = step["text"].as_str().unwrap_or_default();
    let mut line = match step["op"].as_str().unwrap_or_default() {
        "CLICK" => format!("Clicked “{target}”"),
        "TYPE_TEXT" => format!("Typed “{text}” into “{target}”"),
        "SELECT" => format!("Chose “{target}”"),
        "OPEN_URL" => format!("Opened {text}"),
        "MENU" => format!("Chose menu {target}"),
        "PRESS_ENTER" | "PRESS_RETURN" => "Pressed Return".to_string(),
        "SCROLL_DOWN" => "Scrolled down".to_string(),
        "SCROLL_UP" => "Scrolled up".to_string(),
        "WAIT" => "Waited for the page".to_string(),
        "QUICK" => target.to_string(),
        "done" => "Done".to_string(),
        "stuck" => "Stopped".to_string(),
        other => other.to_string(),
    };
    if let Some(note) = step["note"].as_str() {
        line.push_str(" — ");
        line.push_str(note);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_a_typed_step_with_its_text_and_field() {
        let step = json!({ "op": "TYPE_TEXT", "text": "serendipity", "target": { "label": "Search Wiktionary" } });
        assert_eq!(
            describe(&step),
            "Typed “serendipity” into “Search Wiktionary”"
        );
    }

    #[test]
    fn a_stopped_step_keeps_its_reason() {
        let step = json!({ "op": "stuck", "note": "No listed operation can make progress here." });
        assert_eq!(
            describe(&step),
            "Stopped — No listed operation can make progress here."
        );
    }
}
