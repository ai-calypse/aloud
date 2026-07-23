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
    ///
    /// `PermissionRequest` and nothing else. It only fires when the agent
    /// would have asked the user anyway, so permission modes and allowlists
    /// are respected without this crate knowing they exist. Gating on
    /// `PreToolUse` instead was tried and reverted: it fires on calls the
    /// agent would auto-approve, overrides the user's own permission modes,
    /// and freezes the terminal with no visible prompt. See AGENT_NOTCH.md's
    /// diagnosis.
    pub fn is_permission_request(&self) -> bool {
        self.hook_event_name.as_deref() == Some("PermissionRequest")
    }

    /// The questions inside an `AskUserQuestion` call, empty for anything else.
    ///
    /// A question is not its own event: it arrives as a `PermissionRequest`
    /// whose `tool_name` is `AskUserQuestion`, with the questions in
    /// `tool_input.questions`. The parse mirrors the CLI's own rules — a
    /// question missing its text, header, or every option is dropped rather
    /// than rendered half-blank, and an option's description defaults empty.
    pub fn questions(&self) -> Vec<Question> {
        if self.tool_name.as_deref() != Some("AskUserQuestion") {
            return Vec::new();
        }
        let Some(items) = self
            .tool_input
            .as_ref()
            .and_then(|input| input.get("questions"))
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|item| {
                let question = item.get("question")?.as_str()?.to_string();
                let header = item.get("header")?.as_str()?.to_string();
                let options: Vec<QuestionOption> = item
                    .get("options")?
                    .as_array()?
                    .iter()
                    .filter_map(|option| {
                        Some(QuestionOption {
                            label: option.get("label")?.as_str()?.to_string(),
                            description: option
                                .get("description")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect();
                if options.is_empty() {
                    return None;
                }
                Some(Question {
                    question,
                    header,
                    multi_select: item
                        .get("multiSelect")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    options,
                })
            })
            .collect()
    }
}

/// One question from an `AskUserQuestion` call, as the card renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub question: String,
    /// The short chip above the question — "Auth method", "Library".
    pub header: String,
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
}

/// One choice under a question. The freeform "Other" the CLI offers is not in
/// the payload — the UI injects it, matching the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
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
    /// Shown to the model on a deny, so it can say why it stopped — the "No,
    /// and tell Claude what to do differently" text.
    #[serde(default)]
    pub message: Option<String>,
    /// Replaces the tool's input on an allow. This is how an answered
    /// question rides back: the original `AskUserQuestion` input plus an
    /// `answers` map — there is no separate answer message in the protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    /// Permission rules persisted on an allow — the "don't ask again" button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_permissions: Option<serde_json::Value>,
}

impl Reply {
    pub fn allow() -> Self {
        Self {
            decision: Some(Decision::Allow),
            ..Self::default()
        }
    }

    /// Allow, and stop asking for this tool for the rest of the session.
    ///
    /// The rule is synthesized here rather than taken from the payload's
    /// `permission_suggestions`: a bare tool-name rule scoped to `session` is
    /// what "don't ask again this session" means, whatever was suggested.
    pub fn allow_always(tool_name: &str) -> Self {
        Self {
            decision: Some(Decision::Allow),
            updated_permissions: Some(serde_json::json!([{
                "type": "addRules",
                "destination": "session",
                "rules": [{ "toolName": tool_name }],
                "behavior": "allow",
            }])),
            ..Self::default()
        }
    }

    /// Allow with the questions answered: the original `tool_input` gains an
    /// `answers` object mapping each question's text to its answer string
    /// (multi-select answers arrive pre-joined with `", "`).
    pub fn answer(
        original_input: Option<&serde_json::Value>,
        answers: &std::collections::BTreeMap<String, String>,
    ) -> Self {
        let mut input = match original_input {
            Some(serde_json::Value::Object(map)) => map.clone(),
            _ => serde_json::Map::new(),
        };
        input.insert(
            "answers".into(),
            serde_json::to_value(answers).unwrap_or_default(),
        );
        Self {
            decision: Some(Decision::Allow),
            updated_input: Some(serde_json::Value::Object(input)),
            ..Self::default()
        }
    }

    pub fn deny(message: impl Into<String>) -> Self {
        Self {
            decision: Some(Decision::Deny),
            message: Some(message.into()),
            ..Self::default()
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
            Source::Claude => claude_directive(self, decision),
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
fn claude_directive(reply: &Reply, decision: Decision) -> serde_json::Value {
    let inner = match decision {
        Decision::Allow => {
            let mut inner = serde_json::Map::new();
            inner.insert("behavior".into(), "allow".into());
            // Only present when there is something to say: an allow that
            // repeats the input verbatim and an allow that omits it read the
            // same to the agent, and the smaller directive is the safer one.
            if let Some(input) = &reply.updated_input {
                inner.insert("updatedInput".into(), input.clone());
            }
            if let Some(rules) = &reply.updated_permissions {
                inner.insert("updatedPermissions".into(), rules.clone());
            }
            serde_json::Value::Object(inner)
        }
        Decision::Deny => serde_json::json!({
            "behavior": "deny",
            "message": reply.message.as_deref().unwrap_or("Denied in Aloud"),
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

    /// Only `PermissionRequest` gates. A `PreToolUse` must not — it fires on
    /// calls the agent would auto-approve, and gating there overrides the
    /// user's own permission modes and freezes the terminal. The bug this
    /// guards against is a regression to that design.
    #[test]
    fn only_a_permission_request_is_the_blocking_event() {
        let named = |event: &str, tool: &str| Event {
            hook_event_name: Some(event.into()),
            tool_name: Some(tool.into()),
            ..Event::default()
        };
        // Whatever the tool — the filtering is the agent's, not ours.
        assert!(named("PermissionRequest", "Bash").is_permission_request());
        assert!(named("PermissionRequest", "Read").is_permission_request());
        assert!(!named("PreToolUse", "Bash").is_permission_request());
        assert!(!named("PostToolUse", "Bash").is_permission_request());
        assert!(!Event::default().is_permission_request());
    }

    /// "Don't ask again this session" is an allow carrying a session-scoped
    /// rule for the tool. Pinned key by key: a typo'd rule is silently ignored
    /// and the next identical call asks again, which looks like the button
    /// doing nothing.
    #[test]
    fn allow_always_carries_a_session_rule_for_the_tool() {
        let value = Reply::allow_always("Bash")
            .directive(Source::Claude)
            .unwrap();
        let decision = &value["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        let rule = &decision["updatedPermissions"][0];
        assert_eq!(rule["type"], "addRules");
        assert_eq!(rule["destination"], "session");
        assert_eq!(rule["behavior"], "allow");
        assert_eq!(rule["rules"][0]["toolName"], "Bash");
        // A plain allow must not carry the key at all.
        let plain = Reply::allow().directive(Source::Claude).unwrap();
        assert!(plain["hookSpecificOutput"]["decision"]
            .get("updatedPermissions")
            .is_none());
    }

    /// An answered question is an *allow with `updatedInput`* — the original
    /// input plus an `answers` map keyed by question text. There is no other
    /// answer channel in the protocol.
    #[test]
    fn a_question_answer_is_an_allow_with_answers_in_updated_input() {
        let original = serde_json::json!({
            "questions": [{ "question": "Which framework?", "header": "Framework",
                            "options": [{ "label": "React" }] }],
        });
        let mut answers = std::collections::BTreeMap::new();
        answers.insert("Which framework?".to_string(), "React, Vue".to_string());

        let value = Reply::answer(Some(&original), &answers)
            .directive(Source::Claude)
            .unwrap();
        let decision = &value["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        // The original questions survive alongside the new answers key.
        assert_eq!(
            decision["updatedInput"]["questions"][0]["question"],
            "Which framework?"
        );
        assert_eq!(
            decision["updatedInput"]["answers"]["Which framework?"],
            "React, Vue"
        );
    }

    /// The user's "tell Claude what to do differently" text reaches the model.
    #[test]
    fn a_deny_carries_the_users_own_words() {
        let value = Reply::deny("use rg, not grep")
            .directive(Source::Claude)
            .unwrap();
        let decision = &value["hookSpecificOutput"]["decision"];
        assert_eq!(decision["message"], "use rg, not grep");
        assert_eq!(decision["interrupt"], serde_json::json!(false));
    }

    /// Question detection and parsing, pinned against the AskUserQuestion
    /// input shape: a malformed question or option is dropped, not rendered
    /// half-blank, and `multiSelect` defaults off.
    #[test]
    fn ask_user_question_input_parses_to_cards() {
        let event = Event {
            hook_event_name: Some("PermissionRequest".into()),
            tool_name: Some("AskUserQuestion".into()),
            tool_input: Some(serde_json::json!({
                "questions": [
                    { "question": "Which auth method?", "header": "Auth",
                      "multiSelect": true,
                      "options": [
                        { "label": "OAuth", "description": "Redirect flow" },
                        { "label": "API key" },
                      ] },
                    { "question": "No header, dropped",
                      "options": [{ "label": "x" }] },
                    { "question": "No options, dropped", "header": "Empty",
                      "options": [] },
                ],
            })),
            ..Event::default()
        };
        let questions = event.questions();
        assert_eq!(questions.len(), 1);
        let q = &questions[0];
        assert_eq!(q.question, "Which auth method?");
        assert_eq!(q.header, "Auth");
        assert!(q.multi_select);
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.options[0].description, "Redirect flow");
        assert_eq!(q.options[1].description, "");

        // Any other tool has no questions, whatever its input says.
        let bash = Event {
            tool_name: Some("Bash".into()),
            tool_input: Some(serde_json::json!({ "questions": [] })),
            ..Event::default()
        };
        assert!(bash.questions().is_empty());
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
