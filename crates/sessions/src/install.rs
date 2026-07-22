//! Installing our hooks into an agent's config, without breaking anyone else's.
//!
//! Every function here is pure over the config's *contents* — `serde_json` in,
//! `serde_json` out — so the whole installer is testable without touching `~`.
//! That matters more than usual: the thing being edited is a file other tools
//! depend on, and the development machine already has two of them in it.
//!
//! Three rules, each learned from that file:
//!
//! 1. **Append, never rewrite.** Claude Code runs every hook registered for an
//!    event, in order. Coexistence is the normal case, not a conflict.
//! 2. **`PermissionRequest` is exclusive.** Two blocking consumers means two
//!    24-hour blocks and two competing directives, so a foreign owner is
//!    reported and left alone rather than evicted.
//! 3. **Uninstall is exact.** Ours are matched by command path; nothing else is
//!    touched.

use serde_json::{json, Map, Value};

/// The events worth taking on the Claude dialect.
///
/// `PermissionRequest` is deliberately absent: it is the exclusive one, and it
/// is added only by [`install`] when approvals are asked for and nothing else
/// owns it.
pub const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SubagentStart",
    "SubagentStop",
];

/// The blocking event, and the only one that needs negotiating.
pub const PERMISSION_EVENT: &str = "PermissionRequest";

/// A day, matching `aloud_bridge::socket::PERMISSION_TIMEOUT`. The agent parks
/// on the hook's stdout for this long, which is what holds the decision open.
pub const PERMISSION_TIMEOUT_SECS: u64 = 86_400;

/// What an install would do, decided before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Nothing else owns the blocking event.
    None,
    /// Another tool owns `PermissionRequest`. Its command, for the UI to name.
    ///
    /// The user chooses: take it over, or leave approvals off. Neither is
    /// decided here, and nothing is evicted without being asked.
    Foreign(String),
}

/// Whether a hook command is ours.
///
/// By binary name rather than the full path: the sidecar is re-copied on
/// upgrade and its path can change, and an entry that is ours but unrecognised
/// would be orphaned in the user's config forever.
fn is_ours(command: &str) -> bool {
    command.contains("aloud-hook")
}

/// Who owns the blocking event in this config, if anyone.
pub fn permission_conflict(settings: &Value) -> Conflict {
    let entries = settings
        .get("hooks")
        .and_then(|h| h.get(PERMISSION_EVENT))
        .and_then(Value::as_array);
    let Some(entries) = entries else {
        return Conflict::None;
    };
    for matcher in entries {
        for hook in matcher
            .get("hooks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(command) = hook.get("command").and_then(Value::as_str) else {
                continue;
            };
            if !is_ours(command) {
                return Conflict::Foreign(command.to_string());
            }
        }
    }
    Conflict::None
}

/// Adds our entries to a settings document, leaving every other tool's alone.
///
/// `approvals` adds the blocking `PermissionRequest` entry. The caller is
/// expected to have checked [`permission_conflict`] first; this will not evict
/// a foreign owner, it simply coexists, which is why the check has to happen
/// before rather than after.
pub fn install(settings: &Value, command: &str, approvals: bool) -> Value {
    let mut out = settings.clone();
    // A settings file that is not an object is not something to repair — start
    // from one rather than discarding what is there under a wrong assumption.
    if !out.is_object() {
        out = json!({});
    }

    let hooks = out
        .as_object_mut()
        .expect("just ensured an object")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().expect("just ensured an object");

    for event in CLAUDE_EVENTS {
        add_entry(hooks, event, command, None);
    }
    if approvals {
        add_entry(
            hooks,
            PERMISSION_EVENT,
            command,
            Some(PERMISSION_TIMEOUT_SECS),
        );
    }
    out
}

/// Appends one hook entry, replacing only a previous entry of ours.
fn add_entry(hooks: &mut Map<String, Value>, event: &str, command: &str, timeout: Option<u64>) {
    let mut hook = json!({ "type": "command", "command": command });
    if let Some(seconds) = timeout {
        hook["timeout"] = json!(seconds);
    }
    // `matcher: "*"` is what the other consumers of these events use, and an
    // entry without one does not fire for every tool.
    let entry = json!({ "matcher": "*", "hooks": [hook] });

    let list = hooks.entry(event).or_insert_with(|| json!([]));
    if !list.is_array() {
        *list = json!([]);
    }
    let list = list.as_array_mut().expect("just ensured an array");

    // Re-installing must not stack duplicates: drop our previous entries first,
    // then append. Everyone else's keep their position.
    remove_ours(list);
    list.push(entry);
}

/// Strips our entries from a list, and any matcher group left empty by it.
fn remove_ours(list: &mut Vec<Value>) {
    for matcher in list.iter_mut() {
        if let Some(hooks) = matcher.get_mut("hooks").and_then(Value::as_array_mut) {
            hooks.retain(|hook| {
                !hook
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_ours)
            });
        }
    }
    // A group whose only hook was ours is ours to remove; one that still holds
    // another tool's hooks stays exactly as it is.
    list.retain(|matcher| {
        matcher
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|hooks| !hooks.is_empty())
    });
}

/// Removes every entry of ours, and nothing else.
pub fn uninstall(settings: &Value) -> Value {
    let mut out = settings.clone();
    let Some(hooks) = out.get_mut("hooks").and_then(Value::as_object_mut) else {
        return out;
    };
    for (_, list) in hooks.iter_mut() {
        if let Some(list) = list.as_array_mut() {
            remove_ours(list);
        }
    }
    // An event left with no hooks at all is noise in the user's file.
    hooks.retain(|_, list| list.as_array().is_none_or(|l| !l.is_empty()));
    // And so is a `hooks` object with no events left in it — uninstalling
    // should leave the file as it was found, not as an empty shell. Only when
    // we emptied it: a `hooks: {}` the user already had is theirs to keep.
    let emptied = hooks.is_empty()
        && settings
            .get("hooks")
            .and_then(Value::as_object)
            .is_some_and(|before| !before.is_empty());
    if emptied {
        out.as_object_mut().expect("checked above").remove("hooks");
    }
    out
}

/// Which of our hooks are currently installed.
pub fn installed_events(settings: &Value) -> Vec<String> {
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out: Vec<String> = hooks
        .iter()
        .filter(|(_, list)| {
            list.as_array().into_iter().flatten().any(|matcher| {
                matcher
                    .get("hooks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|hook| {
                        hook.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(is_ours)
                    })
            })
        })
        .map(|(event, _)| event.clone())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str =
        "'/Users/y/Library/Application Support/Aloud/bin/aloud-hook' --source claude";

    /// Captured from the development machine: Open Island owns the blocking
    /// event, petdex shares `PreToolUse`. Both must survive an install.
    fn real_world_settings() -> Value {
        json!({
            "hooks": {
                "PermissionRequest": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": "'/Users/y/Library/Application Support/OpenIsland/bin/OpenIslandHooks' --source claude",
                        "timeout": 86400
                    }]
                }],
                "PreToolUse": [
                    { "hooks": [{ "type": "command", "command": "node ~/.petdex/bin/petdex.js bubble pre claude-code" }] },
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "'/Users/y/.../OpenIslandHooks' --source claude" }] }
                ]
            },
            "otherSetting": { "keepMe": true }
        })
    }

    /// The rule that matters most: another tool's hooks are not ours to touch.
    #[test]
    fn installing_leaves_every_other_tools_entry_intact() {
        let before = real_world_settings();
        let after = install(&before, OURS, false);

        // petdex and Open Island both still registered on PreToolUse...
        let pre = after["hooks"]["PreToolUse"].as_array().unwrap();
        let commands: Vec<&str> = pre
            .iter()
            .flat_map(|m| m["hooks"].as_array().unwrap())
            .map(|h| h["command"].as_str().unwrap())
            .collect();
        assert!(commands.iter().any(|c| c.contains("petdex")));
        assert!(commands.iter().any(|c| c.contains("OpenIslandHooks")));
        assert!(commands.iter().any(|c| c.contains("aloud-hook")));

        // ...and settings that are nothing to do with hooks are untouched.
        assert_eq!(after["otherSetting"]["keepMe"], json!(true));
    }

    /// Two blocking consumers means two 24h blocks and two competing
    /// directives, so this is reported and the user decides.
    #[test]
    fn a_foreign_permission_owner_is_detected_not_evicted() {
        let settings = real_world_settings();
        let Conflict::Foreign(command) = permission_conflict(&settings) else {
            panic!("expected the foreign owner to be found");
        };
        assert!(command.contains("OpenIslandHooks"));

        // Even installing with approvals on must not remove it.
        let after = install(&settings, OURS, true);
        let owners: Vec<&str> = after["hooks"]["PermissionRequest"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["hooks"].as_array().unwrap())
            .map(|h| h["command"].as_str().unwrap())
            .collect();
        assert!(owners.iter().any(|c| c.contains("OpenIslandHooks")));
    }

    /// Our own entry is not a conflict with ourselves.
    #[test]
    fn our_own_permission_entry_is_not_a_foreign_owner() {
        let settings = install(&json!({}), OURS, true);
        assert_eq!(permission_conflict(&settings), Conflict::None);
    }

    /// The blocking entry needs its timeout, or the agent gives up long before
    /// a human has answered.
    #[test]
    fn the_permission_entry_carries_the_full_day_timeout() {
        let after = install(&json!({}), OURS, true);
        let hook = &after["hooks"][PERMISSION_EVENT][0]["hooks"][0];
        assert_eq!(hook["timeout"], json!(86_400));
        // The non-blocking events must not carry one.
        assert!(after["hooks"]["Stop"][0]["hooks"][0]
            .get("timeout")
            .is_none());
    }

    /// Approvals default off, so nothing is written for the blocking event
    /// unless it was asked for.
    #[test]
    fn without_approvals_the_blocking_event_is_not_registered() {
        let after = install(&json!({}), OURS, false);
        assert!(after["hooks"].get(PERMISSION_EVENT).is_none());
        assert_eq!(installed_events(&after).len(), CLAUDE_EVENTS.len());
    }

    /// Re-installing is normal — the path changes on upgrade. It must replace
    /// our entry, not stack a second one beside it.
    #[test]
    fn reinstalling_replaces_our_entry_rather_than_duplicating_it() {
        let once = install(&real_world_settings(), OURS, true);
        let twice = install(&once, "/new/path/aloud-hook --source claude", true);

        let ours: Vec<&str> = twice["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["hooks"].as_array().unwrap())
            .map(|h| h["command"].as_str().unwrap())
            .filter(|c| c.contains("aloud-hook"))
            .collect();
        assert_eq!(ours, vec!["/new/path/aloud-hook --source claude"]);
    }

    /// Uninstall must take ours and leave the file otherwise as it was.
    #[test]
    fn uninstalling_removes_only_our_entries() {
        let before = real_world_settings();
        let after = uninstall(&install(&before, OURS, true));

        assert_eq!(installed_events(&after), Vec::<String>::new());
        // Everything else is byte-for-byte what it was.
        assert_eq!(after, before);
    }

    /// A matcher group holding only our hook goes; one shared with another tool
    /// stays, minus ours.
    #[test]
    fn uninstalling_keeps_a_group_another_tool_still_shares() {
        let shared = json!({
            "hooks": { "PreToolUse": [{
                "matcher": "*",
                "hooks": [
                    { "type": "command", "command": "other-tool" },
                    { "type": "command", "command": OURS }
                ]
            }]}
        });
        let after = uninstall(&shared);
        let hooks = after["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["command"], "other-tool");
    }

    /// A settings file with no hooks section at all is the first-run case.
    #[test]
    fn an_empty_config_installs_cleanly() {
        let after = install(&json!({}), OURS, true);
        assert_eq!(installed_events(&after).len(), CLAUDE_EVENTS.len() + 1);
        assert_eq!(uninstall(&after), json!({}));
    }

    /// Uninstalling something never installed must not invent structure.
    #[test]
    fn uninstalling_a_clean_config_changes_nothing() {
        let settings = real_world_settings();
        assert_eq!(uninstall(&settings), settings);
    }
}
