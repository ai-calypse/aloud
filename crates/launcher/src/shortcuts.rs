use anyhow::{Context, Result};
use std::process::Command;

/// The names in the user's Shortcuts library, read fresh — unlike the agent's
/// tool list, the panel is looked at after the user makes a shortcut and must
/// show it. Empty on a Mac with none.
pub fn list_shortcuts() -> Result<Vec<String>> {
    let out = Command::new("shortcuts")
        .arg("list")
        .output()
        .context("running shortcuts")?;
    anyhow::ensure!(
        out.status.success(),
        "shortcuts list failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(parse_list(&String::from_utf8_lossy(&out.stdout)))
}

fn parse_list(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Runs a shortcut and waits for it. Blocks for as long as the shortcut does
/// (one that asks a question waits for the answer), so callers run it off the
/// UI thread.
pub fn run_shortcut(name: &str) -> Result<()> {
    let out = shortcut_command(name)
        .output()
        .context("running shortcuts")?;
    anyhow::ensure!(
        out.status.success(),
        "{}",
        match String::from_utf8_lossy(&out.stderr).trim() {
            "" => format!("{name} did not finish"),
            msg => msg.to_string(),
        }
    );
    Ok(())
}

/// `--` ends option parsing: without it a shortcut named "-h" prints the
/// usage text and "succeeds".
fn shortcut_command(name: &str) -> Command {
    let mut cmd = Command::new("shortcuts");
    cmd.args(["run", "--"]).arg(name);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_never_become_shortcuts() {
        assert_eq!(parse_list("Morning\n\n  Resize  \n"), ["Morning", "Resize"]);
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn a_name_that_looks_like_a_flag_is_still_a_name() {
        let args: Vec<_> = shortcut_command("-h")
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["run", "--", "-h"]);
    }
}
