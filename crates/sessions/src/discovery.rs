//! Finding running agents without their cooperation.
//!
//! Two subprocesses per poll: one `ps` for the processes, one batched `lsof`
//! for the transcripts they hold open. This is the fallback layer — hooks are
//! the truth where they are installed, and this fills the gaps for every
//! session that has none.
//!
//! ponytail: shelling out twice a poll. `libproc` is the upgrade path, and the
//! reason to take it would be the `lsof` call, which is the slow one.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Which agent a process is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

/// A running agent process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub agent: Agent,
    /// The controlling terminal, or `??` for an IDE-hosted session.
    pub tty: String,
}

/// One line of `ps`, parsed.
///
/// Split out from the `ps` call so the filter — the part that decides what
/// counts as an agent — is testable against captured output.
pub fn parse_ps(output: &str) -> Vec<Process> {
    output.lines().filter_map(parse_ps_line).collect()
}

fn parse_ps_line(line: &str) -> Option<Process> {
    let line = line.trim_start();
    let (pid, rest) = line.split_once(char::is_whitespace)?;
    let pid: u32 = pid.parse().ok()?;
    // ppid is in the format so a process tree could be walked later; nothing
    // here needs it yet.
    let (_ppid, rest) = rest.trim_start().split_once(char::is_whitespace)?;
    let (tty, command) = rest.trim_start().split_once(char::is_whitespace)?;
    let command = command.trim();

    let agent = classify(command)?;
    Some(Process {
        pid,
        agent,
        tty: tty.to_string(),
    })
}

/// Whether a command line is an agent worth showing, and which one.
///
/// **The TTY is deliberately not consulted.** `agent-notch` guards on
/// `tty != "??"` to drop headless runs, and on this machine that dropped every
/// session: Claude Code hosted in Cursor/VS Code runs as the extension's
/// `native-binary/claude` with no controlling terminal. The binary's own path
/// is the better filter, so that is what this matches on.
fn classify(command: &str) -> Option<Agent> {
    // The executable is the first word — arguments routinely mention the other
    // agents' names (`--add-dir …/open-vibe-island`, a prompt about codex), and
    // matching anywhere in the line makes every session look like both.
    let exe = command.split_whitespace().next()?;
    let name = exe.rsplit('/').next()?;

    // A wrapper script, a `node …/cli.js`, or an editor open on the source: all
    // match the name and none of them is a running agent.
    let is_agent = |n: &str| name == n || name.starts_with(&format!("{n}-"));

    if is_agent("claude") {
        // Claude Code spawns helpers that share the binary. They are not turns
        // and have no transcript of their own.
        if command.contains("--claude-in-chrome-mcp") {
            return None;
        }
        // The background daemon keeps warm standbys (`--bg-spare`, with a
        // `--bg-pty-host` each) parked in /tmp/cc-daemon-*/spare/ — processes
        // with no conversation, which rendered as rows called "spare".
        if command.contains("--bg-") {
            return None;
        }
        return Some(Agent::Claude);
    }
    if is_agent("codex") {
        return Some(Agent::Codex);
    }
    None
}

/// Every running agent process on the machine.
pub fn processes() -> Vec<Process> {
    let output = Command::new("ps")
        .args(["-Ao", "pid=,ppid=,tty=,command="])
        .output();
    match output {
        Ok(out) => parse_ps(&String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!(%e, "could not list processes");
            Vec::new()
        }
    }
}

/// The transcript files each pid holds open.
///
/// **One `lsof` for every pid, not one per pid.** Per-process spawns blow their
/// own timeout under load — a Codex swarm mid-compile — and every agent then
/// reads as dead. `-Fn` gives a machine-readable stream: `p<pid>` starts a
/// process section, `n<path>` is one open file in it.
pub fn open_transcripts(pids: &[u32]) -> Vec<(u32, Vec<PathBuf>)> {
    if pids.is_empty() {
        return Vec::new();
    }
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let output = Command::new("lsof")
        .args(["-a", "-p", &list, "-Fn"])
        .output();
    match output {
        // lsof exits non-zero when *any* pid is gone, which is normal here —
        // the stdout it did produce is still good.
        Ok(out) => parse_lsof(&String::from_utf8_lossy(&out.stdout), is_transcript),
        Err(e) => {
            tracing::warn!(%e, "could not list open files");
            Vec::new()
        }
    }
}

/// Splits `lsof -Fn` output into per-pid paths, keeping those `wanted` accepts.
///
/// The filter is a parameter because both callers parse the same format and
/// want different lines out of it: transcripts want `.jsonl`, the cwd lookup
/// wants the single directory it asked for.
pub fn parse_lsof(output: &str, wanted: impl Fn(&str) -> bool) -> Vec<(u32, Vec<PathBuf>)> {
    let mut out: Vec<(u32, Vec<PathBuf>)> = Vec::new();
    for line in output.lines() {
        let Some((tag, value)) = line.split_at_checked(1) else {
            continue;
        };
        match tag {
            "p" => {
                if let Ok(pid) = value.parse::<u32>() {
                    out.push((pid, Vec::new()));
                }
            }
            "n" if wanted(value) => {
                // Files before the first `p` line cannot be attributed; lsof
                // does not emit any, but a truncated read could.
                if let Some((_, paths)) = out.last_mut() {
                    paths.push(PathBuf::from(value));
                }
            }
            _ => {}
        }
    }
    out
}

fn is_transcript(path: &str) -> bool {
    path.ends_with(".jsonl")
}

/// The working directory of each pid.
///
/// Claude Code appends to its transcript and closes it again, so on most polls
/// it holds no `.jsonl` open at all and [`open_transcripts`] finds nothing. The
/// cwd is how its transcript is found instead — see [`claude_project_dir`].
pub fn working_dirs(pids: &[u32]) -> Vec<(u32, PathBuf)> {
    if pids.is_empty() {
        return Vec::new();
    }
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // `-d cwd` restricts the descriptor list to the working directory, which is
    // what makes this cheap enough to run beside the transcript call.
    let output = Command::new("lsof")
        .args(["-a", "-p", &list, "-d", "cwd", "-Fn"])
        .output();
    match output {
        // Every path here is the cwd that was asked for, so nothing is filtered.
        Ok(out) => parse_lsof(&String::from_utf8_lossy(&out.stdout), |_| true)
            .into_iter()
            .filter_map(|(pid, mut paths)| paths.pop().map(|p| (pid, p)))
            .collect(),
        Err(e) => {
            tracing::warn!(%e, "could not read working directories");
            Vec::new()
        }
    }
}

/// Where Claude Code keeps the transcripts for a project.
///
/// The directory is the absolute path with every `/` turned into `-`, so
/// `/Users/y/dev/app` becomes `-Users-y-dev-app`. Verified against this
/// machine's `~/.claude/projects`.
pub fn claude_project_dir(home: &Path, cwd: &Path) -> PathBuf {
    let slug: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    home.join(".claude/projects").join(slug)
}

/// One subagent spawned under a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subagent {
    /// What the parent asked it to do, from the `.meta.json` `description`.
    pub label: String,
    /// Its own transcript, so its liveness and model can be read like any other.
    pub transcript: PathBuf,
}

/// The subagents of a Claude Code session.
///
/// They live in `<transcript-without-ext>/subagents/agent-*.jsonl`, each with a
/// sibling `agent-*.meta.json` carrying `agentType` and `description` — the
/// label the parent gave the task, which is better than anything the child's
/// own transcript names itself.
pub fn subagents(transcript: &Path) -> Vec<Subagent> {
    let dir = transcript.with_extension("").join("subagents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Pair each transcript with its meta; the meta is the label source.
        if path.extension().is_some_and(|x| x == "jsonl") {
            let meta = path.with_extension("meta.json");
            let label = read_meta_description(&meta).unwrap_or_else(|| "subagent".to_string());
            out.push(Subagent {
                label,
                transcript: path,
            });
        }
    }
    // Stable order, or the disclosure reshuffles between polls.
    out.sort_by(|a, b| a.transcript.cmp(&b.transcript));
    out
}

/// The `description` from a subagent's `.meta.json`, if any.
fn read_meta_description(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("description")
        .and_then(|d| d.as_str())
        .map(str::to_string)
}

/// The most recently modified transcript in a directory.
///
/// ponytail: newest-by-mtime, which is one `readdir` and no file reads. It can
/// name the wrong file when two sessions share a project — the transcript's own
/// `sessionId` is the fix, and it costs a read of every candidate.
pub fn newest_transcript(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from this machine: every Claude Code session hosted in Cursor
    /// has `tty = ??`. agent-notch's `tty != "??"` guard dropped all of them,
    /// which is the bug this crate exists to not have.
    const PS_SAMPLE: &str = concat!(
        "20825  7281 ??       /Users/y/.cursor/extensions/anthropic.claude-code-2.1.215-darwin-arm64/resources/native-binary/claude --output-format stream-json --add-dir /Users/y/Documents/Development/apps/open-vibe-island\n",
        "21082  7281 ??       /Users/y/.cursor/extensions/anthropic.claude-code-2.1.215-darwin-arm64/resources/native-binary/claude --claude-in-chrome-mcp\n",
        "31337 31336 s004     codex --model gpt-5\n",
        "  501     1 ??       /usr/libexec/secinitd\n",
        "42424 31336 s005     vim crates/sessions/src/discovery.rs\n",
        // The daemon's warm standbys, captured from this machine: same binary,
        // no conversation. Rendered as rows called "spare" until filtered.
        "93963     1 ??       /opt/homebrew/Caskroom/claude-code/2.1.197/claude --bg-pty-host /tmp/cc-daemon-501/c7c4fdd6/spare/343a808e.pty.sock 200 50\n",
        "93981     1 ??       /opt/homebrew/Caskroom/claude-code/2.1.197/claude --bg-spare /tmp/cc-daemon-501/c7c4fdd6/spare/343a808e.claim.sock\n",
    );

    #[test]
    fn an_ide_hosted_session_is_not_dropped_for_having_no_tty() {
        let found = parse_ps(PS_SAMPLE);
        let claude = found
            .iter()
            .find(|p| p.pid == 20825)
            .expect("the IDE-hosted session is found");
        assert_eq!(claude.agent, Agent::Claude);
        assert_eq!(claude.tty, "??");
    }

    #[test]
    fn helpers_and_unrelated_processes_are_not_sessions() {
        let found = parse_ps(PS_SAMPLE);
        let pids: Vec<u32> = found.iter().map(|p| p.pid).collect();
        // The chrome-mcp helper shares the binary but is not a turn.
        assert!(!pids.contains(&21082));
        assert!(!pids.contains(&501));
        // An editor open on this very file mentions no agent name in argv[0].
        assert!(!pids.contains(&42424));
        // The daemon's warm standbys are not sessions — dropping them is what
        // keeps "spare" rows out of the notch.
        assert!(!pids.contains(&93963));
        assert!(!pids.contains(&93981));
        assert_eq!(pids, vec![20825, 31337]);
    }

    /// argv mentions other agents constantly — `--add-dir …/open-vibe-island`
    /// in the sample. Matching the whole line makes one session read as several.
    #[test]
    fn an_agent_named_in_arguments_is_not_a_second_session() {
        let found = parse_ps(PS_SAMPLE);
        assert_eq!(found.iter().filter(|p| p.pid == 20825).count(), 1);
        assert_eq!(found[0].agent, Agent::Claude);
    }

    #[test]
    fn codex_is_found_by_its_own_name() {
        let found = parse_ps(PS_SAMPLE);
        let codex = found.iter().find(|p| p.pid == 31337).unwrap();
        assert_eq!(codex.agent, Agent::Codex);
        assert_eq!(codex.tty, "s004");
    }

    /// The batched call is what keeps a swarm from timing out, so the split
    /// back into per-pid groups is the thing that must not drift.
    #[test]
    fn lsof_chunks_split_on_pid_markers() {
        let output = "p20825\nfcwd\nn/Users/y/.claude/projects/a/one.jsonl\nn/dev/null\np31337\nn/Users/y/.codex/sessions/two.jsonl\nn/tmp/notes.txt\n";
        let parsed = parse_lsof(output, is_transcript);
        assert_eq!(
            parsed,
            vec![
                (
                    20825,
                    vec![PathBuf::from("/Users/y/.claude/projects/a/one.jsonl")]
                ),
                (
                    31337,
                    vec![PathBuf::from("/Users/y/.codex/sessions/two.jsonl")]
                ),
            ]
        );
    }

    /// Claude Code appends and closes, so it usually holds nothing open. An
    /// empty group is a normal answer, not a parse failure.
    #[test]
    fn a_process_holding_no_transcript_still_gets_a_group() {
        assert_eq!(
            parse_lsof("p20825\nn/dev/null\n", is_transcript),
            vec![(20825, vec![])]
        );
    }

    /// The bug this catches: the cwd lookup reuses this parser, and filtering
    /// its output for `.jsonl` throws away every directory it returns — which
    /// silently disabled the fallback for every Claude Code session.
    #[test]
    fn a_cwd_lookup_keeps_directories_the_transcript_filter_would_drop() {
        let output = "p20825\nfcwd\nn/Users/y/Documents/aloud-tauri\n";
        assert!(parse_lsof(output, is_transcript)[0].1.is_empty());
        assert_eq!(
            parse_lsof(output, |_| true),
            vec![(20825, vec![PathBuf::from("/Users/y/Documents/aloud-tauri")])]
        );
    }

    /// Verified against this machine's `~/.claude/projects`: the cwd's absolute
    /// path with every separator turned into a dash.
    #[test]
    fn a_cwd_maps_to_its_claude_project_directory() {
        let dir = claude_project_dir(
            Path::new("/Users/yakshgandhi"),
            Path::new("/Users/yakshgandhi/Documents/Development/apps/aloud-tauri"),
        );
        assert_eq!(
            dir,
            Path::new("/Users/yakshgandhi/.claude/projects/-Users-yakshgandhi-Documents-Development-apps-aloud-tauri")
        );
    }

    /// The subagent dir sits beside the transcript with its extension dropped,
    /// and each child's label comes from its `.meta.json` `description`. Both
    /// verified against a real `~/.claude/projects/.../subagents/` layout.
    #[test]
    fn subagents_are_read_with_their_meta_description() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(&transcript, "{}").unwrap();
        let subs = transcript.with_extension("").join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        std::fs::write(subs.join("agent-a.jsonl"), "{}").unwrap();
        std::fs::write(
            subs.join("agent-a.meta.json"),
            r#"{"agentType":"Explore","description":"Map the settings"}"#,
        )
        .unwrap();
        // A transcript with no meta falls back to a generic label, not a skip.
        std::fs::write(subs.join("agent-b.jsonl"), "{}").unwrap();

        let found = subagents(&transcript);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].label, "Map the settings");
        assert_eq!(found[1].label, "subagent");
        // A session with no subagents dir is the common case, not an error.
        assert!(subagents(Path::new("/tmp/nope.jsonl")).is_empty());
    }

    /// The newest `.jsonl` is the live session, and nothing else in the folder
    /// counts — a stray file must not be handed back as a transcript.
    #[test]
    fn the_newest_transcript_wins_and_other_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.jsonl"), "{}").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        // Enough of a gap to clear the filesystem's mtime resolution; writing
        // the two back to back can leave them with the same stamp.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let new = dir.path().join("new.jsonl");
        std::fs::write(&new, "{}").unwrap();

        assert_eq!(newest_transcript(dir.path()), Some(new));
        assert_eq!(newest_transcript(Path::new("/nonexistent")), None);
    }
}
