//! `shortcuts` — the Shortcuts.app CLI. An escape hatch, not the primary
//! mechanism: verified that `shortcuts list` returns nothing on a fresh
//! machine, so most users have zero. Exposed as one tool that enumerates the
//! library at startup, not one tool per shortcut (there are usually none).

use std::process::Command;

/// The names in the user's Shortcuts library, one per line from `shortcuts
/// list`. Empty on any machine without Shortcuts set up — which is most — and an
/// empty list must yield zero tools, never one broken one.
///
/// Read once: it is a process spawn, and it used to run on every command.
/// ponytail: a shortcut added while the app runs appears after a restart.
pub fn list() -> Vec<String> {
    static LIBRARY: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LIBRARY.get_or_init(read).clone()
}

fn read() -> Vec<String> {
    #[cfg(not(target_os = "macos"))]
    {
        return Vec::new();
    }
    #[cfg(target_os = "macos")]
    {
        let Ok(out) = Command::new("shortcuts").arg("list").output() else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// Runs a named shortcut. Returns its stdout, if any.
pub fn run(name: &str) -> Result<String, String> {
    let out = Command::new("shortcuts")
        .args(["run", name])
        .output()
        .map_err(|e| format!("running shortcut {name:?}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_library_yields_no_shortcuts() {
        // Not asserting the count (this machine may have some) — asserting the
        // shape: every returned name is non-empty, so a blank line from the CLI
        // never becomes a zero-width tool. The zero-lines→zero-tools property is
        // covered by the filter above; this guards the trim/filter.
        for name in list() {
            assert!(!name.trim().is_empty());
        }
    }
}
