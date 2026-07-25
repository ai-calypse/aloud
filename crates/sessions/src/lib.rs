//! Which coding agents are running, and what each one is doing.
//!
//! Two sources feed **one table**. Hooks are the truth where they are
//! installed: [`SessionStore::apply`] folds each bridge event in, keyed by the
//! agent's own session id — the identity nothing outside the process can
//! observe. Discovery is the fallback: a poll of `ps` and `lsof` finds the
//! processes, and their transcripts say the rest, which is what makes an agent
//! visible with nothing installed into anyone's config. Where both see the
//! same session, discovery *enriches* the hook row (title, running tool,
//! subagents) rather than appearing beside it — two stores merged at emit
//! time was tried, and every bug it grew came from the merge.
//!
//! macOS only for now. Windows returns no sessions, the way [`aloud_notch`]
//! returns no screens — the discovery is entirely `ps`/`lsof` shaped.
//!
//! ```no_run
//! let mut store = aloud_sessions::SessionStore::default();
//! store.poll();
//! for session in store.sessions() {
//!     println!("{} {}", session.agent.label(), session.busy);
//! }
//! ```

pub mod discovery;
pub mod install;
pub mod scan;
pub mod transcript;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub use discovery::{Agent, Process};
pub use scan::{Scan, Tool};

/// A running agent, as far as anything outside its process can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub pid: u32,
    pub agent: Agent,
    /// The agent's own id for the conversation, where hooks reported one.
    /// This is what the UI keys a row on when present — stable across polls,
    /// process restarts, and the pid changing under a resume.
    pub session_id: Option<String>,
    /// Where the transcript was read from, when one was found.
    pub transcript: Option<PathBuf>,
    /// The directory the agent is working in — its identity in the UI, since
    /// one agent per project is the normal case.
    pub cwd: Option<String>,
    /// The model's title for the session, else the user's last prompt. What a
    /// row is called.
    pub title: Option<String>,
    /// The user's most recent prompt, verbatim. Kept apart from `title`: the
    /// model's `ai-title` is written once at session start and never moves,
    /// and a row labelled "You:" must show what the user actually said last.
    pub prompt: Option<String>,
    /// The tool running right now, if the transcript ends mid-call.
    pub running: Option<Tool>,
    /// The model in use, vendor prefix stripped: `opus-4-8`, `gpt-5.6-sol`.
    pub model: Option<String>,
    /// The git branch, from the transcript. With the cwd's folder name it
    /// says what the session is working on: `aloud-tauri @ feat/agent-notch`.
    pub branch: Option<String>,
    /// When the session was last active, as an ISO-8601 string — the row's
    /// `· now` / `· 10m` tag is computed from this.
    pub last_activity: Option<String>,
    /// Subagents spawned under this session, each already scanned.
    pub subagents: Vec<Subagent>,
    /// Mid-turn: the transcript moved within [`scan::BUSY_WINDOW`], or a hook
    /// said a turn is open and `Stop` has not.
    pub busy: bool,
    /// A recent one-shot state — `"denied"` or `"session_ended"` — while still
    /// fresh, else `None`. The webview shows its cat over the derived stage.
    pub transient: Option<String>,
}

/// A subagent, flattened the same way as its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subagent {
    /// What the parent asked it to do.
    pub label: String,
    pub model: Option<String>,
    pub running: Option<Tool>,
    pub busy: bool,
}

impl Session {
    /// What to call this row: the model's title, else the prompt, else the
    /// folder being worked in, else the agent's own name.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .or_else(|| {
                self.cwd
                    .as_deref()
                    .and_then(|c| Path::new(c).file_name())
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| self.agent.label().to_string())
    }
}

/// One transcript read: the tail scan plus its subagents. Bundled so `merge`
/// takes a single reader closure, which is what keeps its tests from having to
/// stage a `subagents/` directory on disk.
pub struct Read {
    pub scan: Scan,
    pub subagents: Vec<Subagent>,
}

/// How many consecutive polls a process must be missing before its session is
/// dropped.
///
/// `ps` occasionally misses a live process, and a session blinking out and back
/// is worse than one lingering ~6s. Ported from `agent-notch`, where a bare
/// "gone this poll" flickered constantly. Applies to hook sessions too — this
/// is the reaper for the session `kill -9`ed with no `SessionEnd`.
const GRACE_POLLS: u8 = 2;

/// How long a finished subagent stays in the disclosure before retiring.
/// Long enough to read "completed", short enough that a day-long session is
/// not thirty dead rows.
const SUBAGENT_WINDOW: Duration = Duration::from_secs(300);

/// A one-shot event the row should reflect briefly, then forget — the two
/// stages with no standing state to read: a denial and a session ending. Both
/// fire once and are gone, so they live as a timestamped marker that decays
/// rather than a field the poll would have to keep clearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transient {
    /// The user denied a permission the agent asked for.
    Denied,
    /// `SessionEnd` arrived — the row is kept this long so the "walking away"
    /// state is seen, then reaped.
    Ended,
}

impl Transient {
    /// The webview stage string; matches a `public/cats/<name>/` folder.
    fn as_str(self) -> &'static str {
        match self {
            Transient::Denied => "denied",
            Transient::Ended => "session_ended",
        }
    }
}

/// How long a [`Transient`] marker shows before it decays — long enough to
/// read the cat, short enough that an ended row does not linger.
const TRANSIENT_TTL: Duration = Duration::from_secs(6);

/// How long a hook session whose process was never discovered survives
/// without a hook event.
///
/// The escape hatch for a fork whose binary name `classify` does not know:
/// its hooks work, its process is invisible, and reaping it on process
/// absence would flicker it out mid-turn. `SessionEnd` still removes it
/// immediately; this only bounds the case where that never arrives.
const NO_PROCESS_TTL: Duration = Duration::from_secs(30 * 60);

/// Sessions across polls, so a process missing once does not vanish.
#[derive(Default)]
pub struct SessionStore {
    live: HashMap<u32, Tracked>,
    /// Sessions hooks have told us about, keyed by the agent's own session id.
    hooked: HashMap<String, Hooked>,
    /// Incremental transcript readers, shared by both sources.
    tail: scan::Tail,
}

#[derive(Debug, Clone)]
struct Tracked {
    session: Session,
    /// Consecutive polls this pid has been absent from `ps`.
    missed: u8,
}

/// What hooks have said about one session, plus the enrichment the poll reads
/// from its transcript. The hook fields move the instant an event arrives; the
/// scan fields refresh once a poll — so a tool call shows the moment it
/// starts, and the title catches up within seconds.
struct Hooked {
    agent: Agent,
    cwd: Option<String>,
    transcript: Option<PathBuf>,
    /// The user's latest prompt, the title fallback until an `ai-title` lands.
    prompt: Option<String>,
    model: Option<String>,
    /// The tool a `PreToolUse` announced, cleared by `PostToolUse`/`Stop`.
    tool: Option<Tool>,
    /// A turn is open: set by prompts and tool calls, cleared by `Stop`.
    busy: bool,
    /// A one-shot state and when it fired; decays after [`TRANSIENT_TTL`]. An
    /// `Ended` marker also keeps the row alive for that window (see the reaper).
    transient: Option<(Transient, SystemTime)>,
    last_event: SystemTime,
    /// Enrichment from the transcript, refreshed each poll.
    scan: Scan,
    scan_busy: bool,
    subagents: Vec<Subagent>,
    /// The discovered process this session claimed, when one matched — its
    /// row is suppressed so the conversation is not shown twice.
    pid: Option<u32>,
    /// Whether a process ever matched: gates the 2-poll reap, so a session
    /// whose process discovery cannot see falls back to [`NO_PROCESS_TTL`].
    seen_process: bool,
    missed: u8,
}

impl Hooked {
    fn new(agent: Agent, now: SystemTime) -> Self {
        Hooked {
            agent,
            cwd: None,
            transcript: None,
            prompt: None,
            model: None,
            tool: None,
            busy: false,
            transient: None,
            last_event: now,
            scan: Scan::default(),
            scan_busy: false,
            subagents: Vec::new(),
            pid: None,
            seen_process: false,
            missed: 0,
        }
    }

    /// The merged row: hook fields win where they are fresher by nature
    /// (identity, the open turn, the announced tool), scan fields where the
    /// transcript knows more (title, model, subagents).
    ///
    /// `now` resolves the [`Transient`] marker's freshness — a decayed one
    /// reads as `None` even between the polls that prune it.
    fn session(&self, id: &str, now: SystemTime) -> Session {
        Session {
            pid: self.pid.unwrap_or(0),
            agent: self.agent,
            session_id: Some(id.to_string()),
            transcript: self.transcript.clone(),
            cwd: self.scan.cwd.clone().or_else(|| self.cwd.clone()),
            title: self
                .scan
                .title
                .clone()
                .or_else(|| self.prompt.clone())
                .or_else(|| self.scan.prompt.clone()),
            // The hook's copy lands the moment the prompt is submitted; the
            // scan's catches up a poll later.
            prompt: self.prompt.clone().or_else(|| self.scan.prompt.clone()),
            running: self.tool.clone().or_else(|| self.scan.running.clone()),
            model: self.scan.model.clone().or_else(|| self.model.clone()),
            branch: self.scan.branch.clone(),
            // Both are the same fixed ISO shape, so string max is time max —
            // whichever of "last hook event" and "last transcript record" is
            // newer drives the row's relative tag.
            last_activity: std::cmp::max(
                self.scan.last_activity.clone(),
                scan::format_timestamp(self.last_event),
            ),
            subagents: self.subagents.clone(),
            busy: self.busy || self.scan_busy,
            transient: self.fresh_transient(now).map(|t| t.as_str().to_string()),
        }
    }

    /// The transient marker if it is still within [`TRANSIENT_TTL`] of `now`,
    /// else `None`. A clock that jumped backwards counts as fresh, not expired.
    fn fresh_transient(&self, now: SystemTime) -> Option<Transient> {
        self.transient
            .and_then(|(t, at)| match now.duration_since(at) {
                Ok(age) if age >= TRANSIENT_TTL => None,
                _ => Some(t),
            })
    }

    /// Record a one-shot state, replacing any earlier one — a denial or the
    /// session ending is the newest thing worth showing.
    fn mark(&mut self, transient: Transient, now: SystemTime) {
        self.transient = Some((transient, now));
    }
}

impl SessionStore {
    /// One discovery cycle: find the processes, read their transcripts, merge
    /// into what is already known, and reconcile the hook sessions against it.
    pub fn poll(&mut self) {
        let now = SystemTime::now();
        let found = discovery::processes();
        self.poll_at(found, now);
    }

    /// The cycle with the world passed in, so tests control the clock and the
    /// process list. Transcripts are still real files — tempdirs in tests.
    fn poll_at(&mut self, found: Vec<Process>, now: SystemTime) {
        self.tail.begin();
        {
            let Self { live, tail, .. } = self;
            merge_into(live, found, now, |path| read_transcript(tail, path, now));
        }
        self.reconcile_hooked(now);
        self.tail.prune();
    }

    /// Folds one hook event in. `now` is a parameter for the same reason it is
    /// everywhere else here: the reap rules are testable without waiting.
    pub fn apply(&mut self, event: &aloud_bridge::Event, now: SystemTime) {
        let Some(id) = event.session_id.clone() else {
            return;
        };
        let name = event.hook_event_name.as_deref().unwrap_or_default();
        // The session is over. Rather than dropping the row at once, mark it
        // ended and busy-off so the "walking away" cat shows; the reaper
        // removes it once the marker decays. A row with no prior hook state
        // (SessionEnd is the first thing we heard) is nothing worth flashing.
        if name == "SessionEnd" {
            if let Some(hooked) = self.hooked.get_mut(&id) {
                hooked.busy = false;
                hooked.tool = None;
                hooked.mark(Transient::Ended, now);
            }
            return;
        }

        let agent = match event.source {
            Some(aloud_bridge::Source::Codex) => Agent::Codex,
            _ => Agent::Claude,
        };
        let hooked = self
            .hooked
            .entry(id)
            .or_insert_with(|| Hooked::new(agent, now));
        hooked.last_event = now;
        // An event is proof of life, whatever the process list said last poll.
        hooked.missed = 0;

        if event.cwd.is_some() {
            hooked.cwd = event.cwd.clone();
        }
        if let Some(path) = &event.transcript_path {
            hooked.transcript = Some(PathBuf::from(path));
        }
        if let Some(model) = event.model.as_deref() {
            hooked.model = Some(model.strip_prefix("claude-").unwrap_or(model).to_string());
        }
        if event.prompt.is_some() {
            hooked.prompt = event.prompt.clone();
        }

        // The turn's shape: a prompt or a tool call means a turn is open,
        // `Stop` closes it. `PostToolUse` clears the tool without clearing
        // busy — the agent is thinking about what it just read.
        match name {
            "PreToolUse" | "PermissionRequest" => {
                hooked.busy = true;
                if let Some(tool) = event.tool_name.clone() {
                    let detail = event
                        .tool_input
                        .as_ref()
                        .and_then(|input| scan::headline(&tool, input));
                    hooked.tool = Some(Tool { name: tool, detail });
                }
            }
            "PostToolUse" => hooked.tool = None,
            "UserPromptSubmit" | "SessionStart" | "SubagentStart" => hooked.busy = true,
            "Stop" => {
                hooked.busy = false;
                hooked.tool = None;
            }
            _ => {}
        }
    }

    /// Flag a hook session as just-denied, so its next emit shows the rejected
    /// cat. A no-op for an unknown id — the permission's session may have been
    /// reaped, and there is nothing to mark. The marker decays on its own.
    pub fn mark_denied(&mut self, session_id: &str, now: SystemTime) {
        if let Some(hooked) = self.hooked.get_mut(session_id) {
            hooked.mark(Transient::Denied, now);
        }
    }

    /// Refreshes each hook session's transcript enrichment and decides its
    /// liveness against the processes this poll found.
    fn reconcile_hooked(&mut self, now: SystemTime) {
        let Self { live, hooked, tail } = self;
        for session in hooked.values_mut() {
            if let Some(transcript) = session.transcript.clone() {
                if let Ok(scan) = tail.scan(&transcript) {
                    session.scan_busy = scan.busy(now);
                    session.scan = scan;
                }
                session.subagents = read_subagents(tail, &transcript, now);
            }

            // Claim the discovered process for this conversation: the exact
            // transcript first, the working directory as the fallback.
            let matched = live.values().find_map(|tracked| {
                let same_transcript = session.transcript.is_some()
                    && session.transcript == tracked.session.transcript;
                let same_cwd = session.cwd.is_some() && session.cwd == tracked.session.cwd;
                (same_transcript || same_cwd).then_some(tracked.session.pid)
            });
            match matched {
                Some(pid) => {
                    session.pid = Some(pid);
                    session.seen_process = true;
                    session.missed = 0;
                }
                // Only counts against sessions a process once vouched for;
                // never-seen ones ride on NO_PROCESS_TTL below.
                None if session.seen_process => session.missed += 1,
                None => {}
            }
            if session.missed > 0 {
                // A gone process cannot still be mid-tool.
                session.busy = false;
                session.tool = None;
            }
        }
        hooked.retain(|_, session| {
            // An ended session is kept only while its marker is fresh — the row
            // shows "walking away" for the window, then it goes. This overrides
            // the liveness rules below: `SessionEnd` already declared it done.
            if matches!(session.transient, Some((Transient::Ended, _))) {
                return session.fresh_transient(now) == Some(Transient::Ended);
            }
            if session.seen_process {
                session.missed < GRACE_POLLS
            } else {
                now.duration_since(session.last_event)
                    .is_ok_and(|age| age < NO_PROCESS_TTL)
                    || now < session.last_event
            }
        });
    }

    /// The merge itself, with the world passed in — so the grace rule is
    /// testable without spawning a process or writing a transcript.
    #[cfg(test)]
    fn merge(
        &mut self,
        found: Vec<Process>,
        now: SystemTime,
        read: impl FnMut(&Path) -> Option<Read>,
    ) {
        merge_into(&mut self.live, found, now, read);
    }

    /// Every live session: hook rows first (they carry the identity), then
    /// the discovered processes no hook row claims, busy first, stable within.
    ///
    /// Sessions sharing a transcript are folded into one row, and a discovery
    /// row whose transcript or pid a hook session claimed is suppressed —
    /// showing both would be showing the same conversation twice.
    pub fn sessions(&self) -> Vec<Session> {
        self.sessions_at(SystemTime::now())
    }

    /// The transcript path for one session id, resolved directly against the two
    /// source maps. Cheaper than [`sessions`] for a single lookup: no row list
    /// built, no sort, no dedup fold — the caller wants one field, not the table.
    /// Matches an agent's own session id first, then the `pid:<n>` fallback the
    /// row list would have assigned.
    pub fn transcript_for(&self, id: &str) -> Option<PathBuf> {
        if let Some(hooked) = self.hooked.get(id) {
            if let Some(path) = &hooked.transcript {
                return Some(path.clone());
            }
        }
        self.live.values().find_map(|tracked| {
            let s = &tracked.session;
            let matches = s.session_id.as_deref() == Some(id) || format!("pid:{}", s.pid) == id;
            matches.then(|| s.transcript.clone()).flatten()
        })
    }

    /// The rows as of `now` — factored out so tests can drive the clock that
    /// resolves each row's [`Transient`] freshness.
    fn sessions_at(&self, now: SystemTime) -> Vec<Session> {
        let mut out: Vec<Session> = Vec::new();

        // Id-sorted, so the walk is deterministic between polls.
        let mut hooked: Vec<(&String, &Hooked)> = self.hooked.iter().collect();
        hooked.sort_by(|a, b| a.0.cmp(b.0));
        for (id, session) in &hooked {
            out.push(session.session(id, now));
        }

        // By pid first: the map's iteration order varies per poll, and the fold
        // below keeps the first of each group, so an unordered walk would pick
        // a different survivor each time and make the row's details flicker.
        let mut all: Vec<&Session> = self.live.values().map(|t| &t.session).collect();
        all.sort_by_key(|s| s.pid);

        for session in all {
            let claimed = hooked.iter().any(|(_, h)| {
                h.pid == Some(session.pid)
                    || (h.transcript.is_some() && h.transcript == session.transcript)
                    || (h.transcript.is_none() && h.cwd.is_some() && h.cwd == session.cwd)
            });
            if claimed {
                continue;
            }
            // Only a shared transcript folds; two sessions with none of their
            // own are still two agents.
            let duplicate = session
                .transcript
                .as_ref()
                .and_then(|path| out.iter_mut().find(|s| s.transcript.as_ref() == Some(path)));
            match duplicate {
                // The walk is pid-ordered, so the row already holds the lowest;
                // a sibling only contributes its liveness.
                Some(existing) => existing.busy |= session.busy,
                None => out.push(session.clone()),
            }
        }

        out.sort_by(|a, b| (!a.busy, a.pid, &a.session_id).cmp(&(!b.busy, b.pid, &b.session_id)));
        out
    }
}

/// The discovery merge, over the store's process table. A free function so the
/// caller can hold the incremental reader in the same borrow — the closure is
/// the only thing that touches it while this runs.
fn merge_into(
    live: &mut HashMap<u32, Tracked>,
    found: Vec<Process>,
    now: SystemTime,
    mut read: impl FnMut(&Path) -> Option<Read>,
) {
    let pids: Vec<u32> = found.iter().map(|p| p.pid).collect();
    let open: HashMap<u32, Vec<PathBuf>> = discovery::open_transcripts(&pids).into_iter().collect();
    let cwds: HashMap<u32, PathBuf> = discovery::working_dirs(&pids).into_iter().collect();
    let home = home_dir();

    for process in found {
        // Three routes, best first. Codex holds its rollout file open, so
        // `lsof` answers directly. Claude Code appends and closes, so it
        // usually shows none: the path it had last poll still stands, and
        // failing that its cwd names the project directory to look in.
        let transcript = open
            .get(&process.pid)
            .and_then(|paths| newest(paths))
            .or_else(|| {
                live.get(&process.pid)
                    .and_then(|t| t.session.transcript.clone())
            })
            .or_else(|| {
                let cwd = cwds.get(&process.pid)?;
                let home = home.as_deref()?;
                discovery::newest_transcript(&discovery::claude_project_dir(home, cwd))
            });

        let read = transcript.as_deref().and_then(&mut read);
        let scan = read.as_ref().map(|r| &r.scan);
        let session = Session {
            pid: process.pid,
            agent: process.agent,
            session_id: None,
            // The transcript's own cwd is the agent's view of where it is
            // working; the process's is the fallback when there is no
            // transcript to read, so a row still names its folder.
            cwd: scan.and_then(|s| s.cwd.clone()).or_else(|| {
                cwds.get(&process.pid)
                    .map(|p| p.to_string_lossy().into_owned())
            }),
            title: scan.and_then(|s| s.title.clone().or_else(|| s.prompt.clone())),
            prompt: scan.and_then(|s| s.prompt.clone()),
            running: scan.and_then(|s| s.running.clone()),
            model: scan.and_then(|s| s.model.clone()),
            branch: scan.and_then(|s| s.branch.clone()),
            last_activity: scan.and_then(|s| s.last_activity.clone()),
            subagents: read
                .as_ref()
                .map(|r| r.subagents.clone())
                .unwrap_or_default(),
            busy: scan.is_some_and(|s| s.busy(now)),
            transcript,
            // Discovery-only rows carry no hook-driven one-shot state.
            transient: None,
        };
        live.insert(process.pid, Tracked { session, missed: 0 });
    }

    // Anything not seen this poll ages out rather than disappearing.
    for (pid, tracked) in live.iter_mut() {
        if !pids.contains(pid) {
            tracked.missed += 1;
            // A process that is gone is not still running a tool.
            tracked.session.busy = false;
            tracked.session.running = None;
        }
    }
    live.retain(|_, t| t.missed < GRACE_POLLS);
}

/// One transcript through the incremental reader, subagents included.
fn read_transcript(tail: &mut scan::Tail, path: &Path, now: SystemTime) -> Option<Read> {
    let scan = tail.scan(path).ok()?;
    let subagents = read_subagents(tail, path, now);
    Some(Read { scan, subagents })
}

/// The subagents of a transcript, each scanned like any other — and retired
/// once finished and quiet for [`SUBAGENT_WINDOW`], so a long session is not
/// a disclosure full of dead rows.
fn read_subagents(tail: &mut scan::Tail, transcript: &Path, now: SystemTime) -> Vec<Subagent> {
    discovery::subagents(transcript)
        .into_iter()
        .filter_map(|sub| {
            let child = tail.scan(&sub.transcript).ok()?;
            child.active_within(now, SUBAGENT_WINDOW).then(|| Subagent {
                label: sub.label,
                model: child.model.clone(),
                running: child.running.clone(),
                busy: child.busy(now),
            })
        })
        .collect()
}

/// The home directory, for locating `~/.claude/projects`.
///
/// `$HOME` rather than the `directories` crate: this crate deliberately has no
/// heavy dependency, and the one path it needs is the one `$HOME` gives.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The most recently modified of several open transcripts.
///
/// A resumed session can hold more than one; the newest is the live turn.
fn newest(paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32) -> Process {
        Process {
            pid,
            agent: Agent::Claude,
            tty: "??".into(),
        }
    }

    /// A blank session with the given pid — the fields the tests do not care
    /// about default, so a new `Session` field does not touch every test.
    fn session(pid: u32) -> Session {
        Session {
            pid,
            agent: Agent::Claude,
            session_id: None,
            transcript: None,
            cwd: None,
            title: None,
            prompt: None,
            branch: None,
            running: None,
            model: None,
            last_activity: None,
            subagents: Vec::new(),
            busy: false,
            transient: None,
        }
    }

    /// Wraps a scan as a read with no subagents — most tests do not exercise
    /// them, and threading `Read` through by hand would be noise.
    fn read(scan: Scan) -> Read {
        Read {
            scan,
            subagents: Vec::new(),
        }
    }

    /// A hook event of the given name for the given session, Claude-shaped.
    fn event(name: &str, session: &str) -> aloud_bridge::Event {
        aloud_bridge::Event {
            source: Some(aloud_bridge::Source::Claude),
            hook_event_name: Some(name.into()),
            session_id: Some(session.into()),
            ..aloud_bridge::Event::default()
        }
    }

    /// `ps` occasionally misses a live process. Dropping a session on the first
    /// absence makes the indicator flicker; it must survive exactly one.
    #[test]
    fn a_dead_process_survives_one_missed_poll() {
        let mut store = SessionStore::default();
        let now = SystemTime::now();
        store.merge(vec![process(1)], now, |_| None);
        assert_eq!(store.sessions().len(), 1);

        store.merge(vec![], now, |_| None);
        assert_eq!(store.sessions().len(), 1, "one missed poll is forgiven");

        store.merge(vec![], now, |_| None);
        assert!(store.sessions().is_empty(), "two is not");
    }

    /// A process that came back before the grace ran out must not carry the
    /// missed count into the next disappearance.
    #[test]
    fn a_returning_process_resets_its_grace() {
        let mut store = SessionStore::default();
        let now = SystemTime::now();
        store.merge(vec![process(1)], now, |_| None);
        store.merge(vec![], now, |_| None);
        store.merge(vec![process(1)], now, |_| None);
        store.merge(vec![], now, |_| None);
        assert_eq!(store.sessions().len(), 1, "the count restarted");
    }

    /// Claude Code closes its transcript between writes, so `lsof` shows none
    /// on most polls. Forgetting the path each time loses the title and the
    /// running tool for the whole session.
    #[test]
    fn a_known_transcript_survives_a_poll_that_finds_no_open_file() {
        let mut store = SessionStore::default();
        let now = SystemTime::now();
        let path = PathBuf::from("/tmp/one.jsonl");
        store.live.insert(
            1,
            Tracked {
                session: Session {
                    transcript: Some(path.clone()),
                    ..session(1)
                },
                missed: 0,
            },
        );

        let mut asked = Vec::new();
        store.merge(vec![process(1)], now, |p| {
            asked.push(p.to_path_buf());
            Some(read(Scan {
                title: Some("still here".into()),
                ..Scan::default()
            }))
        });
        assert_eq!(asked, vec![path]);
        assert_eq!(store.sessions()[0].title.as_deref(), Some("still here"));
    }

    /// A read of a turn that is happening right now.
    fn mid_turn() -> Read {
        read(Scan {
            last_activity: Some(ACTIVE.into()),
            running: Some(Tool {
                name: "Bash".into(),
                detail: Some("cargo test".into()),
            }),
            ..Scan::default()
        })
    }

    /// A stamp, and the instant it means. `busy` compares the two, so the tests
    /// pass the matching `now` rather than reaching for the wall clock. The
    /// pairing is pinned by `timestamps_parse_to_the_right_instant` in `scan`.
    const ACTIVE: &str = "2026-07-22T21:19:09.394Z";
    fn active_at() -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_784_755_149)
    }

    /// Seeds a session whose transcript path is known, so a test closure can
    /// answer differently per pid.
    fn with_transcript(store: &mut SessionStore, pid: u32) {
        store.live.insert(
            pid,
            Tracked {
                session: Session {
                    transcript: Some(PathBuf::from(format!("/proc/{pid}.jsonl"))),
                    ..session(pid)
                },
                missed: 0,
            },
        );
    }

    /// A gone process cannot still be mid-tool: the row would sit there
    /// claiming `Bash: cargo test` for the whole grace window.
    #[test]
    fn a_missing_process_stops_claiming_to_be_busy() {
        let mut store = SessionStore::default();
        let now = active_at();
        with_transcript(&mut store, 1);
        store.merge(vec![process(1)], now, |_| Some(mid_turn()));
        assert!(store.sessions()[0].busy);
        assert!(store.sessions()[0].running.is_some());

        store.merge(vec![], now, |_| None);
        let session = &store.sessions()[0];
        assert!(!session.busy);
        assert_eq!(session.running, None);
    }

    /// The busy row is the one the user is waiting on, so it goes to the top —
    /// and the rest hold a stable order, or the list reshuffles every 3s poll.
    #[test]
    fn busy_sessions_sort_first_and_the_rest_hold_their_order() {
        let mut store = SessionStore::default();
        let now = active_at();
        for pid in [1, 2, 3] {
            with_transcript(&mut store, pid);
        }
        store.merge(vec![process(3), process(1), process(2)], now, |path| {
            // Only pid 2's transcript shows a turn in progress.
            (path == Path::new("/proc/2.jsonl")).then(mid_turn)
        });
        let order: Vec<u32> = store.sessions().iter().map(|s| s.pid).collect();
        assert_eq!(order, vec![2, 1, 3]);
    }

    /// Observed on the development machine: two Claude Code processes in one
    /// project both resolve to that project's newest transcript, because
    /// nothing outside the process maps a pid to a session. Two rows there
    /// would be the same conversation printed twice.
    #[test]
    fn processes_sharing_a_transcript_are_one_row() {
        let mut store = SessionStore::default();
        for pid in [20825, 29514] {
            store.live.insert(
                pid,
                Tracked {
                    session: Session {
                        transcript: Some(PathBuf::from("/proc/shared.jsonl")),
                        title: Some("Add tab system".into()),
                        // Only the sibling is mid-turn.
                        busy: pid == 29514,
                        ..session(pid)
                    },
                    missed: 0,
                },
            );
        }
        let sessions = store.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].pid, 20825, "the lowest pid is the stable row");
        assert!(sessions[0].busy, "a busy sibling makes the row busy");
    }

    /// The model and subagents ride through the merge onto the session, so the
    /// row can show a `model · time` tag and a subagent disclosure.
    #[test]
    fn a_session_carries_its_model_and_subagents() {
        let mut store = SessionStore::default();
        let now = active_at();
        with_transcript(&mut store, 1);
        store.merge(vec![process(1)], now, |_| {
            Some(Read {
                scan: Scan {
                    model: Some("opus-4-8".into()),
                    last_activity: Some(ACTIVE.into()),
                    ..Scan::default()
                },
                subagents: vec![Subagent {
                    label: "Map the settings".into(),
                    model: Some("sonnet-5".into()),
                    running: None,
                    busy: true,
                }],
            })
        });
        let session = &store.sessions()[0];
        assert_eq!(session.model.as_deref(), Some("opus-4-8"));
        assert_eq!(session.last_activity.as_deref(), Some(ACTIVE));
        assert_eq!(session.subagents.len(), 1);
        assert_eq!(session.subagents[0].label, "Map the settings");
        assert!(session.subagents[0].busy);
    }

    /// Two agents that resolved no transcript are still two agents — the fold
    /// keys on a shared path, not on the absence of one.
    #[test]
    fn processes_without_a_transcript_are_not_folded_together() {
        let mut store = SessionStore::default();
        let now = active_at();
        store.merge(vec![process(1), process(2)], now, |_| None);
        assert_eq!(store.sessions().len(), 2);
    }

    #[test]
    fn a_row_falls_back_from_title_to_folder_to_agent() {
        let base = session(1);
        assert_eq!(base.label(), "claude");
        let with_cwd = Session {
            cwd: Some("/Users/y/Documents/aloud-tauri".into()),
            ..base.clone()
        };
        assert_eq!(with_cwd.label(), "aloud-tauri");
        let with_title = Session {
            title: Some("Fix the parser".into()),
            ..with_cwd
        };
        assert_eq!(with_title.label(), "Fix the parser");
    }

    /// The identity hooks add: a row keyed by the agent's own session id, that
    /// discovery could never produce.
    #[test]
    fn a_hook_event_creates_a_session_keyed_by_its_own_id() {
        let mut store = SessionStore::default();
        let now = active_at();
        let mut start = event("SessionStart", "s1");
        start.cwd = Some("/Users/y/dev/app".into());
        start.model = Some("claude-opus-4-8".into());
        store.apply(&start, now);

        let sessions = store.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id.as_deref(), Some("s1"));
        assert_eq!(sessions[0].model.as_deref(), Some("opus-4-8"));
        assert!(sessions[0].busy, "a session that just started is mid-turn");
    }

    /// The turn's shape through hook events: `PreToolUse` announces the tool,
    /// `PostToolUse` clears it while the turn stays open, `Stop` closes it.
    #[test]
    fn hook_events_open_and_close_the_turn() {
        let mut store = SessionStore::default();
        let now = active_at();
        let mut pre = event("PreToolUse", "s1");
        pre.tool_name = Some("Bash".into());
        pre.tool_input = Some(serde_json::json!({"command": "cargo test"}));
        store.apply(&pre, now);
        let row = &store.sessions()[0];
        assert!(row.busy);
        assert_eq!(
            row.running.as_ref().unwrap().detail.as_deref(),
            Some("cargo test")
        );

        store.apply(&event("PostToolUse", "s1"), now);
        let row = &store.sessions()[0];
        assert!(row.busy, "thinking about what it just read");
        assert_eq!(row.running, None);

        store.apply(&event("Stop", "s1"), now);
        assert!(!store.sessions()[0].busy);
    }

    /// `SessionEnd` keeps the row briefly with an `ended` marker so the
    /// "walking away" cat shows, then the reaper drops it once it decays.
    #[test]
    fn a_session_end_shows_ended_then_reaps() {
        let mut store = SessionStore::default();
        let now = active_at();
        store.apply(&event("SessionStart", "s1"), now);
        assert_eq!(store.sessions().len(), 1);

        store.apply(&event("SessionEnd", "s1"), now);
        // Still present, flagged ended, and no longer busy.
        let rows = store.sessions_at(now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].transient.as_deref(), Some("session_ended"));
        assert!(!rows[0].busy);

        // Past the TTL, the reaper (run inside the poll) drops it.
        let later = now + TRANSIENT_TTL + Duration::from_secs(1);
        store.reconcile_hooked(later);
        assert!(store.sessions_at(later).is_empty());
    }

    /// A `SessionEnd` for an id we never heard of is a no-op — nothing to flag,
    /// nothing to keep.
    #[test]
    fn a_session_end_for_an_unknown_id_creates_no_row() {
        let mut store = SessionStore::default();
        store.apply(&event("SessionEnd", "ghost"), active_at());
        assert!(store.sessions().is_empty());
    }

    /// A denial flags the row so the rejected cat shows, and the marker decays.
    #[test]
    fn a_denial_flags_the_row_then_clears() {
        let mut store = SessionStore::default();
        let now = active_at();
        store.apply(&event("SessionStart", "s1"), now);

        store.mark_denied("s1", now);
        assert_eq!(
            store.sessions_at(now)[0].transient.as_deref(),
            Some("denied")
        );

        // A denial does not remove the row — the session is still live.
        let later = now + TRANSIENT_TTL + Duration::from_secs(1);
        store.reconcile_hooked(later);
        let rows = store.sessions_at(later);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].transient, None);
    }

    /// The bug the single store exists to prevent: a discovered process whose
    /// transcript a hook session claims must fold into that row — with the
    /// scan's enrichment on it — not appear beside it, and not erase it.
    #[test]
    fn a_hooked_row_claims_its_discovered_process_and_its_enrichment() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("s1.jsonl");
        std::fs::write(
            &transcript,
            format!(
                "{}\n{}\n",
                r#"{"type":"ai-title","aiTitle":"Fix the parser"}"#,
                format_args!(
                    r#"{{"type":"assistant","timestamp":"{ACTIVE}","cwd":"/Users/y/dev/app","message":{{"model":"claude-opus-4-8","content":[]}}}}"#
                ),
            ),
        )
        .unwrap();
        let now = active_at();

        let mut store = SessionStore::default();
        let mut start = event("SessionStart", "s1");
        start.transcript_path = Some(transcript.to_string_lossy().into_owned());
        store.apply(&start, now);
        // Discovery found the process behind it, resolving the same file.
        store.live.insert(
            7,
            Tracked {
                session: Session {
                    transcript: Some(transcript.clone()),
                    ..session(7)
                },
                missed: 0,
            },
        );

        store.reconcile_hooked(now);
        let sessions = store.sessions();
        assert_eq!(sessions.len(), 1, "one conversation, one row");
        assert_eq!(sessions[0].session_id.as_deref(), Some("s1"));
        assert_eq!(sessions[0].pid, 7, "the process folded into the hook row");
        assert_eq!(
            sessions[0].title.as_deref(),
            Some("Fix the parser"),
            "the transcript's enrichment reached the hook row"
        );
        assert_eq!(sessions[0].model.as_deref(), Some("opus-4-8"));
    }

    /// A `kill -9` sends no `SessionEnd`. Once its process vanishes, the hook
    /// row follows the same two-poll grace as everything else.
    #[test]
    fn a_hooked_session_whose_process_died_is_reaped() {
        let mut store = SessionStore::default();
        let now = active_at();
        let mut start = event("SessionStart", "s1");
        start.cwd = Some("/Users/y/dev/app".into());
        store.apply(&start, now);
        store.live.insert(
            7,
            Tracked {
                session: Session {
                    cwd: Some("/Users/y/dev/app".into()),
                    ..session(7)
                },
                missed: 0,
            },
        );
        store.reconcile_hooked(now);
        assert_eq!(store.sessions()[0].pid, 7, "matched by cwd");

        store.live.clear();
        store.reconcile_hooked(now);
        assert_eq!(store.sessions().len(), 1, "one missed poll is forgiven");
        store.reconcile_hooked(now);
        assert!(store.sessions().is_empty(), "two is not");
    }

    /// A hook session whose process discovery never saw — a fork with a binary
    /// name `classify` does not know — must not flicker on process absence.
    /// It lives on event recency instead, and still dies eventually.
    #[test]
    fn a_never_discovered_hook_session_rides_on_event_recency() {
        let mut store = SessionStore::default();
        let now = active_at();
        store.apply(&event("SessionStart", "s1"), now);
        // Many polls with no matching process: stays.
        for _ in 0..10 {
            store.reconcile_hooked(now);
        }
        assert_eq!(store.sessions().len(), 1);

        // But silence past the TTL retires it.
        let later = now + NO_PROCESS_TTL + Duration::from_secs(1);
        store.reconcile_hooked(later);
        assert!(store.sessions().is_empty());
    }

    /// A finished subagent retires after its window, or a long session shows
    /// every subagent it ever ran.
    #[test]
    fn finished_subagents_retire_after_their_window() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("t.jsonl");
        std::fs::write(&transcript, "{}\n").unwrap();
        let subs = transcript.with_extension("").join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        std::fs::write(
            subs.join("agent-a.jsonl"),
            format!(r#"{{"type":"assistant","timestamp":"{ACTIVE}","message":{{"content":[]}}}}"#)
                + "\n",
        )
        .unwrap();

        let mut tail = scan::Tail::default();
        let now = active_at();
        assert_eq!(read_subagents(&mut tail, &transcript, now).len(), 1);
        let later = now + SUBAGENT_WINDOW + Duration::from_secs(1);
        let mut tail = scan::Tail::default();
        assert!(read_subagents(&mut tail, &transcript, later).is_empty());
    }
}
