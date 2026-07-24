//! Starting a coding agent from Aloud: a git worktree checked out to a chosen
//! branch, then `claude`/`codex` opened in it in a real Terminal tab.
//!
//! This app has only ever *discovered* agent processes someone else launched
//! (`aloud_sessions`). This is the one place it starts one itself. The point of
//! opening a visible Terminal — rather than piping a headless child — is that
//! the very same `ps`/`lsof` discovery then finds it like any human-launched
//! session, so nothing downstream needs a new code path.
//!
//! **macOS only.** Opening a Terminal tab is AppleScript (`osascript`), with no
//! equivalent this app targets on Windows yet; [`spawn`] is a no-op there, the
//! way `aloud_volume` and `aloud_permissions` stub their macOS bodies. The whole
//! feature therefore fails closed on Windows rather than half-creating a
//! worktree with no way to reach the agent — see [`start_coding_agent`].

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which CLI to open in the worktree. Serde/specta so this is the one type the
/// Tauri command and its TS binding use — no parallel enum to keep in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    /// The executable name, which is also what `aloud_sessions::discovery`
    /// classifies on — so a session opened here reads back as the same agent.
    fn binary(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

/// A local git repo the composer can offer, so the user picks instead of typing
/// a path. `name` is the folder; `path` is what `start_coding_agent` takes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalRepo {
    pub name: String,
    pub path: String,
}

/// The default places to look for repos, under `$HOME`. Mirrors
/// `selection::installed_apps`'s fixed-roots approach.
///
/// ponytail: a fixed list, not a setting. A configurable roots list is the
/// upgrade path once someone keeps repos somewhere unusual; until then the
/// composer's free-text field is the escape hatch for anything off this list.
const REPO_ROOTS: &[&str] = &["Documents/Development", "dev", "Projects", "src", "code"];

/// Every git repo found under [`REPO_ROOTS`], sorted by name, cached for the
/// scan's lifetime. A repo is any directory holding a `.git` — the `origin/main`
/// requirement is left to `create_worktree`, which surfaces git's own error
/// rather than paying a `git` spawn per repo just to pre-filter the list.
pub fn local_repos() -> Vec<LocalRepo> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<LocalRepo>)>> =
        std::sync::Mutex::new(None);
    const TTL: std::time::Duration = std::time::Duration::from_secs(60);

    if let Ok(cache) = CACHE.lock() {
        if let Some((at, repos)) = cache.as_ref() {
            if at.elapsed() < TTL {
                return repos.clone();
            }
        }
    }
    // Not holding the lock across the scan — two composers opening at once
    // duplicate the walk rather than block each other, like `installed_apps`.
    let repos = scan_repos();
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((std::time::Instant::now(), repos.clone()));
    }
    repos
}

/// Resolves [`REPO_ROOTS`] against `$HOME` and walks them. Split from the walk
/// so the walk is testable against a tempdir without touching the environment.
fn scan_repos() -> Vec<LocalRepo> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    let roots: Vec<PathBuf> = REPO_ROOTS.iter().map(|r| home.join(r)).collect();
    scan_roots(&roots)
}

/// Whether `dir` is a repo's main checkout — its `.git` is a *directory*.
///
/// A linked worktree's `.git` is a *file* (`gitdir: …`), and our own spawns
/// create sibling worktrees like `<repo>-<branch>`. Requiring a `.git`
/// directory drops those, so a worktree never shows up in the list as if it
/// were a separate repo.
fn is_repo_root(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

/// Walks each root two levels deep (`<root>/<repo>` and `<root>/<group>/<repo>`,
/// so `~/dev/foo` and `~/dev/work/foo` both land) collecting main checkouts.
fn scan_roots(roots: &[PathBuf]) -> Vec<LocalRepo> {
    let mut repos: Vec<LocalRepo> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue; // a root that does not exist is the normal case
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if is_repo_root(&path) {
                push_repo(&mut repos, &mut seen, path);
            } else {
                // One more level: a group folder like `~/dev/work/`.
                if let Ok(inner) = std::fs::read_dir(&path) {
                    for child in inner.flatten() {
                        let child = child.path();
                        if child.is_dir() && is_repo_root(&child) {
                            push_repo(&mut repos, &mut seen, child);
                        }
                    }
                }
            }
        }
    }
    repos.sort_by(|a, b| a.name.cmp(&b.name));
    repos
}

/// Adds a repo once, keyed on its full path.
fn push_repo(repos: &mut Vec<LocalRepo>, seen: &mut std::collections::HashSet<String>, path: PathBuf) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let path_str = path.to_string_lossy().into_owned();
    if seen.insert(path_str.clone()) {
        repos.push(LocalRepo {
            name: name.to_string(),
            path: path_str,
        });
    }
}

/// Where the worktree lands: a sibling of the repo named `<repo>-<branch>`, so
/// all of a repo's worktrees stay discoverable beside it. Matches the sibling
/// convention in open-vibe-island's `docs/worktree-workflow.md`.
///
/// ponytail: one fixed convention, no config. If nesting-under-the-repo or a
/// central worktree dir is ever wanted, this one function is where it changes.
pub fn worktree_path(repo: &Path, branch: &str) -> PathBuf {
    let name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    // Siblings share the parent; a repo at the filesystem root has none, so fall
    // back to placing the worktree beside it under the same (empty) parent.
    let parent = repo.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{name}-{branch}"))
}

/// The local branches in `repo`, newest-committed-to first, for the composer's
/// dropdown. Empty on any git error — a repo we can't read branches from just
/// offers none, and the composer falls back to no selection.
pub fn branches(repo: &Path) -> Vec<String> {
    let output = Command::new("git")
        .current_dir(repo)
        // `--sort=-committerdate` puts the branches you actually touch on top,
        // so the one you want is rarely a scroll away.
        .args([
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .filter(|b| !b.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// `git worktree add <path> <branch>`, run in `repo`, checking out an *existing*
/// branch (not creating one). Fails with git's own stderr attached — this
/// answers a button click, so a silent failure is worse than a loud one.
/// Returns the worktree path on success.
///
/// The worktree dir name replaces `/` in the branch (`feat/login`) with `-`, so
/// a slashed branch still yields a flat sibling directory.
///
/// # Errors
/// If `git` cannot be run, or exits non-zero — most often because the branch is
/// already checked out in another worktree (git refuses to check a branch out
/// twice), which the message carries through verbatim.
pub fn create_worktree(repo: &Path, branch: &str) -> Result<PathBuf> {
    let path = worktree_path(repo, &branch.replace('/', "-"));
    let output = Command::new("git")
        .current_dir(repo)
        .args(["worktree", "add"])
        .arg(&path)
        .arg(branch)
        .output()
        .with_context(|| format!("running git worktree add in {}", repo.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }
    Ok(path)
}

/// Opens a new Terminal tab that `cd`s into `dir` and runs the agent on `task`.
///
/// Both the path and the task are `shell_quote`d into the command, then the
/// whole command is `applescript_escape`d for the `do script` string literal.
/// The task is untrusted (the user typed it, quotes/`$`/backticks and all), so
/// the shell quoting is what makes it inert — a bare `"…"` around it would let a
/// `"` or `$(…)` break out. Two escapes, one per grammar the string passes
/// through: shell, then AppleScript.
///
/// (An earlier version passed the task via an env var on the `osascript`
/// process. That silently sent the *wrong* task: `do script` runs the new tab
/// under the already-running Terminal.app, which does not inherit `osascript`'s
/// environment, so `$VAR` expanded to whatever Terminal.app already had — a
/// stale value from a previous run.)
///
/// ponytail: hardcodes Terminal.app. Discovery (`aloud_sessions`) is
/// terminal-agnostic — it finds a session under iTerm/Ghostty/Alacritty too —
/// so the asymmetry is that we can *find* an agent anywhere but only *start* one
/// here. The upgrade path is a per-terminal `do script` dialect keyed on a
/// preference; worth it once someone asks to spawn into their own terminal.
#[cfg(target_os = "macos")]
pub fn spawn(dir: &Path, agent: Agent, task: &str) -> Result<()> {
    let script = terminal_script(dir, agent, task);
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("running osascript to open Terminal")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // First control of Terminal raises a one-time Automation prompt. A
        // denial is -1743 (not authorized); a prompt left unanswered times out
        // as -1712. Either way the fix is the same grant, so both map to one
        // actionable message rather than osascript's raw error code.
        if stderr.contains("-1743") || stderr.contains("-1712") {
            anyhow::bail!(
                "Aloud needs permission to control Terminal — grant it under \
                 System Settings → Privacy & Security → Automation, then try again"
            );
        }
        anyhow::bail!("could not open Terminal: {}", stderr.trim());
    }
    Ok(())
}

/// ponytail: Windows is a no-op, like `aloud_volume`'s. The real path would open
/// Windows Terminal (`wt.exe`), worth writing only once a Windows user exists to
/// test it — and [`start_coding_agent`] refuses to create a worktree there so
/// this never silently leaves one stranded.
#[cfg(not(target_os = "macos"))]
pub fn spawn(_dir: &Path, _agent: Agent, _task: &str) -> Result<()> {
    Ok(())
}

/// The one entry point the Tauri command calls: worktree the chosen `branch` →
/// open the agent on `task`. Returns the branch so the caller can show it at
/// once, without waiting on discovery's next poll to name the new session.
///
/// The `task` is the agent's instruction, verbatim — it no longer names the
/// branch (the user picks that), so the two are fully decoupled.
///
/// # Errors
/// If `repo` is not a directory, `branch` is blank, the worktree cannot be
/// created (most often the branch is already checked out elsewhere), or Terminal
/// cannot be opened. On Windows it refuses up front rather than creating a
/// worktree [`spawn`] cannot open.
pub fn start_coding_agent(repo: &Path, branch: &str, task: &str, agent: Agent) -> Result<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (repo, branch, task, agent);
        anyhow::bail!("starting a coding agent is only supported on macOS");
    }
    #[cfg(target_os = "macos")]
    {
        anyhow::ensure!(repo.is_dir(), "not a directory: {}", repo.display());
        anyhow::ensure!(!branch.trim().is_empty(), "pick a branch to work on");
        let dir = create_worktree(repo, branch)?;
        spawn(&dir, agent, task)?;
        Ok(branch.to_string())
    }
}

/// The AppleScript that opens a Terminal tab running the agent. Split out from
/// [`spawn`] so the two layers of escaping — shell for the command, AppleScript
/// for the `do script` literal — are testable without opening a real Terminal.
#[cfg(target_os = "macos")]
fn terminal_script(dir: &Path, agent: Agent, task: &str) -> String {
    let command = format!(
        "cd {} && {} {}",
        shell_quote(&dir.to_string_lossy()),
        agent.binary(),
        shell_quote(task),
    );
    format!(
        r#"tell application "Terminal"
    activate
    do script "{}"
end tell"#,
        applescript_escape(&command)
    )
}

/// Wraps a string in single quotes for the shell, escaping any single quote the
/// POSIX way (`'\''`). Used for the `cd` path Terminal runs.
#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Escapes `\` and `"` so a string is safe inside an AppleScript `"…"` literal.
#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', r#"\""#)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worktree is a sibling `<repo>-<branch>`, whatever the repo's depth.
    #[test]
    fn worktree_path_is_a_named_sibling() {
        assert_eq!(
            worktree_path(Path::new("/Users/y/dev/aloud-tauri"), "add-login"),
            Path::new("/Users/y/dev/aloud-tauri-add-login")
        );
        // A trailing slash must not swallow the repo name.
        assert_eq!(
            worktree_path(Path::new("/Users/y/dev/app/"), "fix"),
            Path::new("/Users/y/dev/app-fix")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        assert_eq!(shell_quote("/a/b"), "'/a/b'");
        assert_eq!(shell_quote("/a's/b"), r"'/a'\''s/b'");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_escape_closes_no_literal_early() {
        assert_eq!(applescript_escape(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(applescript_escape(r"a\b"), r"a\\b");
    }

    /// The task the user typed must reach the command verbatim, not via a `$VAR`
    /// the spawned shell wouldn't inherit — the bug where "hello" ran as a stale
    /// "add a readme". And a task with shell metacharacters stays inert.
    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_script_embeds_the_task_and_neutralizes_it() {
        let s = terminal_script(Path::new("/repo"), Agent::Claude, "hello");
        // The literal task is in the script; no environment indirection.
        assert!(s.contains("claude 'hello'"), "{s}");
        assert!(!s.contains("ALOUD_AGENT_TASK") && !s.contains('$'), "{s}");

        // Quotes, `$`, and backticks in the task cannot break out of the shell
        // single-quote (they become inert text), and the `"` is then escaped
        // again for the AppleScript literal.
        let evil = terminal_script(Path::new("/repo"), Agent::Codex, r#"a"; rm -rf ~ #"#);
        assert!(evil.contains(r#"'a\"; rm -rf ~ #'"#), "{evil}");
    }

    /// The walk finds repos one and two levels down, skips non-repo dirs, and
    /// de-dupes — the logic that would silently list the wrong thing.
    #[test]
    fn scan_roots_finds_repos_at_both_depths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let git = |p: &Path| std::fs::create_dir_all(p.join(".git")).unwrap();

        git(&root.join("direct")); // <root>/direct        — depth 1
        git(&root.join("work/nested")); // <root>/work/nested   — depth 2
        std::fs::create_dir_all(root.join("plain/notes")).unwrap(); // no .git anywhere
        // A repo that also contains a nested repo: only the outer one counts,
        // the walk stops descending once it finds a `.git`.
        git(&root.join("outer"));
        git(&root.join("outer/inner"));
        // A linked worktree: `.git` is a FILE, not a dir. It must NOT be listed
        // as a repo — this is the `DynamicNotch-hello` pollution bug.
        std::fs::create_dir_all(root.join("direct-wt")).unwrap();
        std::fs::write(root.join("direct-wt/.git"), "gitdir: /somewhere\n").unwrap();

        let mut names: Vec<String> = scan_roots(&[root]).into_iter().map(|r| r.name).collect();
        names.sort();
        assert_eq!(names, vec!["direct", "nested", "outer"]);
    }

    /// A root that does not exist is the normal case (most people have only one
    /// of the default dev dirs), not an error.
    #[test]
    fn scan_roots_ignores_missing_roots() {
        assert!(scan_roots(&[PathBuf::from("/nonexistent/xyz")]).is_empty());
    }
}
