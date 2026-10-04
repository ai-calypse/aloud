//! `audit.jsonl` in the app data directory: one line per gate decision and per
//! write that ran — the tool, the app, what was decided, whether it worked,
//! when. Never arguments or message text: the file says what the agent was let
//! do, not what anyone wrote.
//!
//! ponytail: appended forever, no rotation. A line is ~120 bytes, so a year of
//! heavy use is a few MB; rotate by size if it ever matters.

use anyhow::Result;
use serde::Serialize;
use std::{io::Write, path::Path};

#[derive(Debug, Serialize)]
pub struct Entry<'a> {
    pub tool: &'a str,
    pub app: &'a str,
    /// "allowed" or "declined" at the gate, "ran" for a write that executed.
    pub decision: &'a str,
    /// Whether a write that ran worked; absent for a gate decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// Appends `entry` to `<app-data>/Aloud/audit.jsonl`, stamped with the time.
pub fn append(entry: &Entry) -> Result<()> {
    append_to(&aloud_notes::data_dir()?.join("audit.jsonl"), entry)
}

fn append_to(path: &Path, entry: &Entry) -> Result<()> {
    #[derive(Serialize)]
    struct Line<'a> {
        #[serde(flatten)]
        entry: &'a Entry<'a>,
        at: String,
    }
    let mut line = serde_json::to_string(&Line {
        entry,
        at: chrono::Local::now().to_rfc3339(),
    })?;
    line.push('\n');
    // One write per line, so concurrent writes from one round's calls never
    // interleave inside a line.
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The log must say what happened without repeating what was said: no
    /// field beyond the five, whatever the caller had in hand.
    #[test]
    fn each_entry_is_one_line_of_tool_app_decision_ok_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");
        let gate = Entry {
            tool: "SLACK_CHAT_POST_MESSAGE",
            app: "slack",
            decision: "allowed",
            ok: None,
        };
        let ran = Entry {
            decision: "ran",
            ok: Some(true),
            ..gate
        };
        append_to(&path, &gate).unwrap();
        append_to(&path, &ran).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        let keys = |v: &serde_json::Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(keys(&lines[0]), ["app", "at", "decision", "tool"]);
        assert_eq!(keys(&lines[1]), ["app", "at", "decision", "ok", "tool"]);
        assert_eq!(lines[1]["ok"], true);
    }
}
