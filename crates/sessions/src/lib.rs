//! Which coding agents are running, and what each one is doing.
//!
//! Read-only, and nothing here asks the agents' permission: a poll of `ps` and
//! `lsof` finds the processes, and their transcripts say the rest. That makes
//! it work on an agent that was already running before Aloud started, with
//! nothing installed into anyone's config.
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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use discovery::{Agent, Process};
pub use scan::{Scan, Tool};

/// A running agent, as far as anything outside its process can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub pid: u32,
    pub agent: Agent,
    /// Where the transcript was read from, when one was found.
    pub transcript: Option<PathBuf>,
    /// The directory the agent is working in — its identity in the UI, since
    /// one agent per project is the normal case.
    pub cwd: Option<String>,
    /// The model's title for the session, else the user's last prompt. What a
    /// row is called.
    pub title: Option<String>,
    /// The tool running right now, if the transcript ends mid-call.
    pub running: Option<Tool>,
    /// Mid-turn: the transcript moved within [`scan::BUSY_WINDOW`].
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

/// How many consecutive polls a process must be missing before its session is
/// dropped.
///
/// `ps` occasionally misses a live process, and a session blinking out and back
/// is worse than one lingering ~6s. Ported from `agent-notch`, where a bare
/// "gone this poll" flickered constantly.
const GRACE_POLLS: u8 = 2;

/// Sessions across polls, so a process missing once does not vanish.
#[derive(Default)]
pub struct SessionStore {
    live: HashMap<u32, Tracked>,
}

#[derive(Debug, Clone)]
struct Tracked {
    session: Session,
    /// Consecutive polls this pid has been absent from `ps`.
    missed: u8,
}

impl SessionStore {
    /// One discovery cycle: find the processes, read their transcripts, and
    /// merge into what is already known.
    pub fn poll(&mut self) {
        self.merge(discovery::processes(), SystemTime::now(), |path| {
            scan::scan(path).ok()
        });
    }

    /// The merge itself, with the world passed in — so the grace rule is
    /// testable without spawning a process or writing a transcript.
    fn merge(
        &mut self,
        found: Vec<Process>,
        now: SystemTime,
        mut read: impl FnMut(&Path) -> Option<Scan>,
    ) {
        let pids: Vec<u32> = found.iter().map(|p| p.pid).collect();
        let open: HashMap<u32, Vec<PathBuf>> =
            discovery::open_transcripts(&pids).into_iter().collect();
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
                    self.live
                        .get(&process.pid)
                        .and_then(|t| t.session.transcript.clone())
                })
                .or_else(|| {
                    let cwd = cwds.get(&process.pid)?;
                    let home = home.as_deref()?;
                    discovery::newest_transcript(&discovery::claude_project_dir(home, cwd))
                });

            let scan = transcript.as_deref().and_then(&mut read);
            let session = Session {
                pid: process.pid,
                agent: process.agent,
                // The transcript's own cwd is the agent's view of where it is
                // working; the process's is the fallback when there is no
                // transcript to read, so a row still names its folder.
                cwd: scan.as_ref().and_then(|s| s.cwd.clone()).or_else(|| {
                    cwds.get(&process.pid)
                        .map(|p| p.to_string_lossy().into_owned())
                }),
                title: scan
                    .as_ref()
                    .and_then(|s| s.title.clone().or_else(|| s.prompt.clone())),
                running: scan.as_ref().and_then(|s| s.running.clone()),
                busy: scan.as_ref().is_some_and(|s| s.busy(now)),
                transcript,
            };
            self.live
                .insert(process.pid, Tracked { session, missed: 0 });
        }

        // Anything not seen this poll ages out rather than disappearing.
        for (pid, tracked) in self.live.iter_mut() {
            if !pids.contains(pid) {
                tracked.missed += 1;
                // A process that is gone is not still running a tool.
                tracked.session.busy = false;
                tracked.session.running = None;
            }
        }
        self.live.retain(|_, t| t.missed < GRACE_POLLS);
    }

    /// Every live session, busy first, then stably by pid so rows do not swap
    /// places between polls.
    ///
    /// Sessions sharing a transcript are folded into one row. Without hooks
    /// there is nothing that maps a *process* to its *session*: the transcript
    /// path is not in the agent's argv and it holds no descriptor on it, so two
    /// Claude Code processes in one project both resolve to that project's
    /// newest transcript. Showing them as two rows would be showing the same
    /// conversation twice.
    ///
    /// ponytail: one row per transcript, so a second session in the same folder
    /// is invisible until it is the newest. Installing hooks is the fix — the
    /// payload carries `session_id`, which is the identity this cannot observe.
    pub fn sessions(&self) -> Vec<Session> {
        // By pid first: the map's iteration order varies per poll, and the fold
        // below keeps the first of each group, so an unordered walk would pick
        // a different survivor each time and make the row's details flicker.
        let mut all: Vec<&Session> = self.live.values().map(|t| &t.session).collect();
        all.sort_by_key(|s| s.pid);

        let mut out: Vec<Session> = Vec::new();
        for session in all {
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
        out.sort_by_key(|s| (!s.busy, s.pid));
        out
    }
}

/// The home directory, for locating `~/.claude/projects`.
///
/// `$HOME` rather than the `directories` crate: this crate deliberately has no
/// dependency beyond serde, and the one path it needs is the one `$HOME` gives.
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
                    pid: 1,
                    agent: Agent::Claude,
                    transcript: Some(path.clone()),
                    cwd: None,
                    title: None,
                    running: None,
                    busy: false,
                },
                missed: 0,
            },
        );

        let mut asked = Vec::new();
        store.merge(vec![process(1)], now, |p| {
            asked.push(p.to_path_buf());
            Some(Scan {
                title: Some("still here".into()),
                ..Scan::default()
            })
        });
        assert_eq!(asked, vec![path]);
        assert_eq!(store.sessions()[0].title.as_deref(), Some("still here"));
    }

    /// A scan of a turn that is happening right now.
    fn mid_turn() -> Scan {
        Scan {
            last_activity: Some(ACTIVE.into()),
            running: Some(Tool {
                name: "Bash".into(),
                detail: Some("cargo test".into()),
            }),
            ..Scan::default()
        }
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
                    pid,
                    agent: Agent::Claude,
                    transcript: Some(PathBuf::from(format!("/proc/{pid}.jsonl"))),
                    cwd: None,
                    title: None,
                    running: None,
                    busy: false,
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
                        pid,
                        agent: Agent::Claude,
                        transcript: Some(PathBuf::from("/proc/shared.jsonl")),
                        cwd: None,
                        title: Some("Add tab system".into()),
                        running: None,
                        // Only the sibling is mid-turn.
                        busy: pid == 29514,
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
        let base = Session {
            pid: 1,
            agent: Agent::Claude,
            transcript: None,
            cwd: None,
            title: None,
            running: None,
            busy: false,
        };
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
}
