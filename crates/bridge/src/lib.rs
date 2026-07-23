//! The wire between an agent's hooks and the app.
//!
//! Shared by both ends on purpose: `aloud-hook` encodes what it reads on stdin,
//! the app decodes it, and the reply travels back the same way. A format that
//! lived in only one of them would drift, and the failure mode of a drifted
//! directive is silence — the agent proceeds as though nothing were installed,
//! which looks exactly like the feature not working.
//!
//! The payload and directive shapes are each vendor's public hook protocol.
//! Every field name here is one the shipped `claude` binary contains.

pub mod socket;

use serde::{Deserialize, Serialize};

/// Which hook format a payload arrived in.
///
/// One parser covers six agents: the four Claude Code forks and Kimi all speak
/// the Claude dialect. Codex is its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Claude,
    Codex,
}

impl Source {
    /// Parses the `--source` flag. Unknown values are rejected rather than
    /// defaulting: guessing the dialect means encoding a directive in the wrong
    /// shape, which the agent silently ignores.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "claude" => Some(Source::Claude),
            "codex" => Some(Source::Codex),
            _ => None,
        }
    }
}

/// One hook firing, as the sidecar forwards it.
///
/// Unknown fields are kept out of the struct rather than denied: the vendors
/// add fields between releases, and a payload that fails to parse is a hook
/// that does nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Which agent, so the app can encode the reply in the same dialect.
    pub source: Option<Source>,
    #[serde(default)]
    pub hook_event_name: Option<String>,
    /// The agent's own id for the conversation. This is the identity that
    /// process discovery cannot observe — see `aloud-sessions`.
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Arbitrary per-tool JSON. Kept as a `Value` because it is exactly that,
    /// and because the directive has to be able to hand it back unchanged.
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,
    /// What a decision is keyed on. Concurrent requests are normal — a swarm
    /// can ask three times at once — so nothing here assumes one at a time.
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub last_assistant_message: Option<String>,
    /// What makes jump-back possible: which terminal the session is sitting in.
    #[serde(default)]
    pub terminal_app: Option<String>,
    #[serde(default)]
    pub terminal_tty: Option<String>,
    #[serde(default)]
    pub terminal_session_id: Option<String>,
    #[serde(default)]
    pub terminal_title: Option<String>,
}

/// The tools worth gating: the ones that change something. Reads, searches and
/// fetches run silently, or every file the agent opens is a card.
///
/// `PreToolUse` fires on every call, so the filter is what keeps it from being
/// a card storm — see [`Event::wants_gate`].
pub const GATED_TOOLS: &[&str] = &["Bash", "Write", "Edit", "NotebookEdit"];

impl Event {
    /// Whether this event is the blocking kind — the agent is parked on the
    /// hook's stdout waiting for an answer.
    pub fn is_permission_request(&self) -> bool {
        self.hook_event_name.as_deref() == Some("PermissionRequest")
    }

    /// Whether this call should stop for approval.
    ///
    /// Two ways in: a `PermissionRequest` (the agent explicitly asking, when it
    /// fires at all), or a `PreToolUse` for a tool in [`GATED_TOOLS`]. The
    /// latter is what actually carries the gate on current Claude builds, where
    /// `PermissionRequest` is silent.
    pub fn wants_gate(&self) -> bool {
        if self.is_permission_request() {
            return true;
        }
        self.hook_event_name.as_deref() == Some("PreToolUse")
            && self
                .tool_name
                .as_deref()
                .is_some_and(|name| GATED_TOOLS.contains(&name))
    }
}

/// What the app decides about a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
}

/// The app's reply. `None` means "no directive" — the agent proceeds on its own
/// rules, which is also what a missing app produces.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub decision: Option<Decision>,
    /// Shown to the model on a deny, so it can say why it stopped.
    #[serde(default)]
    pub message: Option<String>,
}

impl Reply {
    pub fn allow() -> Self {
        Self {
            decision: Some(Decision::Allow),
            message: None,
        }
    }

    pub fn deny(message: impl Into<String>) -> Self {
        Self {
            decision: Some(Decision::Deny),
            message: Some(message.into()),
        }
    }

    /// The JSON the agent expects on the hook's stdout, or `None` when there is
    /// nothing to say and the hook should print nothing at all.
    ///
    /// Printing `{}` is not the same as printing nothing for every dialect, and
    /// "nothing" is the shape that is safe everywhere.
    pub fn directive(&self, source: Source) -> Option<serde_json::Value> {
        let decision = self.decision?;
        Some(match source {
            Source::Claude => claude_directive(decision, self.message.as_deref()),
            Source::Codex => codex_directive(decision, self.message.as_deref()),
        })
    }

    /// The directive in the shape the *event* expects.
    ///
    /// `PreToolUse` and `PermissionRequest` both gate a call, but spell the
    /// decision differently (`permissionDecision` vs `decision`). Which event
    /// the gate rode in on decides the encoding — a `PreToolUse` answered in
    /// `PermissionRequest`'s shape is silently ignored, which looks exactly like
    /// approvals not working. `PermissionRequest` never fires on current Claude
    /// builds, so `PreToolUse` is the one that carries the gate in practice.
    pub fn directive_for(&self, source: Source, event_name: &str) -> Option<serde_json::Value> {
        let decision = self.decision?;
        if source == Source::Claude && event_name == "PreToolUse" {
            return Some(pre_tool_use_directive(decision, self.message.as_deref()));
        }
        self.directive(source)
    }
}

/// Claude's `PermissionRequest` reply shape.
///
/// `interrupt: false` on a deny is deliberate: it declines this one call and
/// lets the turn continue, where `true` ends the turn outright. Denying a
/// single command should not kill the run.
fn claude_directive(decision: Decision, message: Option<&str>) -> serde_json::Value {
    let inner = match decision {
        Decision::Allow => serde_json::json!({ "behavior": "allow" }),
        Decision::Deny => serde_json::json!({
            "behavior": "deny",
            "message": message.unwrap_or("Denied in Aloud"),
            "interrupt": false,
        }),
    };
    serde_json::json!({
        "continue": true,
        "suppressOutput": true,
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": inner,
        },
    })
}

/// Codex's reply shape — the same decision, a different envelope.
fn codex_directive(decision: Decision, message: Option<&str>) -> serde_json::Value {
    let inner = match decision {
        Decision::Allow => serde_json::json!({ "behavior": "allow" }),
        Decision::Deny => serde_json::json!({
            "behavior": "deny",
            "message": message.unwrap_or("Denied in Aloud"),
        }),
    };
    serde_json::json!({
        "continue": true,
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": inner,
        },
    })
}

/// The older `PreToolUse` shape, which spells the same decision differently.
///
/// Kept separate rather than folded into [`claude_directive`]: the key names
/// differ (`permissionDecision`, not `decision`), and a hook registered on both
/// events has to answer each in its own vocabulary.
pub fn pre_tool_use_directive(decision: Decision, reason: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "continue": true,
        "suppressOutput": true,
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": match decision {
                Decision::Allow => "allow",
                Decision::Deny => "deny",
            },
            "permissionDecisionReason": reason.unwrap_or(""),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typo in any of these key names is a directive the agent ignores, which
    /// is indistinguishable from the feature not being installed. Pinned
    /// against the field names in the shipped `claude` binary.
    #[test]
    fn claude_allow_has_the_exact_key_names_the_agent_reads() {
        let value = Reply::allow().directive(Source::Claude).unwrap();
        assert_eq!(value["continue"], serde_json::json!(true));
        assert_eq!(value["suppressOutput"], serde_json::json!(true));
        let hook = &value["hookSpecificOutput"];
        assert_eq!(hook["hookEventName"], "PermissionRequest");
        assert_eq!(hook["decision"]["behavior"], "allow");
    }

    /// Denying one call must not end the turn — that is what `interrupt` does.
    #[test]
    fn claude_deny_carries_its_message_and_does_not_interrupt() {
        let value = Reply::deny("not this one")
            .directive(Source::Claude)
            .unwrap();
        let decision = &value["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "deny");
        assert_eq!(decision["message"], "not this one");
        assert_eq!(decision["interrupt"], serde_json::json!(false));
    }

    /// No decision means print nothing, not `{}` — the shape that is safe in
    /// every dialect.
    #[test]
    fn no_decision_produces_no_directive_at_all() {
        assert_eq!(Reply::default().directive(Source::Claude), None);
        assert_eq!(Reply::default().directive(Source::Codex), None);
    }

    /// The gate rides in on `PreToolUse` for a write/bash tool, since
    /// `PermissionRequest` is silent on current Claude builds. Reads must not
    /// gate, or every file the agent opens is a card.
    #[test]
    fn a_write_or_bash_pretooluse_wants_the_gate_but_a_read_does_not() {
        let pre = |tool: &str| Event {
            hook_event_name: Some("PreToolUse".into()),
            tool_name: Some(tool.into()),
            ..Event::default()
        };
        assert!(pre("Bash").wants_gate());
        assert!(pre("Write").wants_gate());
        assert!(pre("Edit").wants_gate());
        assert!(!pre("Read").wants_gate());
        assert!(!pre("Grep").wants_gate());
        // A PermissionRequest still gates whatever the tool, when it fires.
        assert!(Event {
            hook_event_name: Some("PermissionRequest".into()),
            tool_name: Some("Read".into()),
            ..Event::default()
        }
        .wants_gate());
        // A PostToolUse never gates — the call already ran.
        assert!(!Event {
            hook_event_name: Some("PostToolUse".into()),
            tool_name: Some("Bash".into()),
            ..Event::default()
        }
        .wants_gate());
    }

    /// The reply's shape follows the event it answers: a `PreToolUse` decision
    /// spelled in `PermissionRequest`'s vocabulary is silently ignored, which
    /// is exactly the bug that made approvals look broken.
    #[test]
    fn a_pretooluse_reply_uses_permission_decision_not_decision() {
        let allow = Reply::allow()
            .directive_for(Source::Claude, "PreToolUse")
            .unwrap();
        let hook = &allow["hookSpecificOutput"];
        assert_eq!(hook["hookEventName"], "PreToolUse");
        assert_eq!(hook["permissionDecision"], "allow");
        assert!(hook.get("decision").is_none());

        let deny = Reply::deny("no")
            .directive_for(Source::Claude, "PreToolUse")
            .unwrap();
        let hook = &deny["hookSpecificOutput"];
        assert_eq!(hook["permissionDecision"], "deny");
        assert_eq!(hook["permissionDecisionReason"], "no");

        // A PermissionRequest still gets the older `decision` shape.
        let pr = Reply::allow()
            .directive_for(Source::Claude, "PermissionRequest")
            .unwrap();
        assert_eq!(pr["hookSpecificOutput"]["decision"]["behavior"], "allow");
    }

    /// `PreToolUse` spells the same decision with different keys.
    #[test]
    fn pre_tool_use_uses_its_own_older_vocabulary() {
        let value = pre_tool_use_directive(Decision::Deny, Some("blocked"));
        let hook = &value["hookSpecificOutput"];
        assert_eq!(hook["hookEventName"], "PreToolUse");
        assert_eq!(hook["permissionDecision"], "deny");
        assert_eq!(hook["permissionDecisionReason"], "blocked");
        // The newer key must not appear, or both are sent and one wins at random.
        assert!(hook.get("decision").is_none());
    }

    /// The vendors add payload fields between releases. An unknown one must not
    /// fail the parse, or a point release turns every hook into a no-op.
    #[test]
    fn an_unknown_payload_field_does_not_fail_the_parse() {
        let json = r#"{"hook_event_name":"PermissionRequest","tool_name":"Bash",
            "session_id":"abc","tool_use_id":"toolu_1","something_new":{"x":1}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(event.is_permission_request());
        assert_eq!(event.tool_name.as_deref(), Some("Bash"));
        assert_eq!(event.session_id.as_deref(), Some("abc"));
    }

    /// `tool_input` is arbitrary per-tool JSON and has to survive untouched.
    #[test]
    fn tool_input_survives_a_round_trip_unchanged() {
        let json = r#"{"tool_name":"Bash","tool_input":{"command":"ls -la","timeout":5000}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        let input = event.tool_input.as_ref().unwrap();
        assert_eq!(input["command"], "ls -la");
        assert_eq!(input["timeout"], 5000);
    }

    #[test]
    fn an_unknown_source_is_rejected_rather_than_guessed() {
        assert_eq!(Source::parse("claude"), Some(Source::Claude));
        assert_eq!(Source::parse("codex"), Some(Source::Codex));
        assert_eq!(Source::parse("cursor"), None);
        assert_eq!(Source::parse(""), None);
    }

    /// The event and reply cross a socket, so both have to survive serde in
    /// both directions.
    #[test]
    fn an_event_and_reply_round_trip_across_the_wire() {
        let event = Event {
            source: Some(Source::Claude),
            hook_event_name: Some("PermissionRequest".into()),
            tool_use_id: Some("toolu_1".into()),
            ..Event::default()
        };
        let back: Event = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(back, event);

        let reply = Reply::deny("no");
        let back: Reply = serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap();
        assert_eq!(back, reply);
    }
}
