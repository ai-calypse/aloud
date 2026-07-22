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

impl Event {
    /// Whether this event is the blocking kind — the agent is parked on the
    /// hook's stdout waiting for an answer.
    pub fn is_permission_request(&self) -> bool {
        self.hook_event_name.as_deref() == Some("PermissionRequest")
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
