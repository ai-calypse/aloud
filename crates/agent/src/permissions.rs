//! What a write needs permission for: the app and action its saved grant is
//! keyed by, whether it is strict, and the named-app rule.
//!
//! The named-app rule is the one silent defence against injected instructions:
//! once outside content (a web page, an email, a Slack message, selected text)
//! is in the run, a write may only go to an app the user named. Anything else
//! asks, even when always-allowed, because the instruction may not be theirs.

use crate::{composio, ToolDef};
use serde_json::Value;
use std::collections::HashSet;

const SEND: &str = "send and share";
const DELETE: &str = "delete";
const PAY: &str = "pay and buy";
const EDIT: &str = "create and edit";

/// Hand tiers that beat Composio's `destructiveHint`, which it sets loosely:
/// a campaign send is public and permanent, a move to Trash is undone in one
/// click.
///
/// ponytail: two rows; hand mappings (preview PR5) replace this table.
const HAND_STRICT: &[(&str, bool)] = &[
    ("GMAIL_MOVE_TO_TRASH", false),
    ("MAILCHIMP_SEND_CAMPAIGN", true),
];

/// Words that mean the user named a Mac app without saying its name.
///
/// ponytail: a hand list per app; the catalogue carrying each action's words is
/// the upgrade when it misses.
const NATIVE_WORDS: &[(&str, &[&str])] = &[
    ("Notes", &["note", "notes"]),
    // Not "message": "my latest slack message" would name Messages, and a
    // Slack message asking for a text would then pass as the user's request.
    ("Messages", &["text", "messages", "sms", "imessage"]),
    ("Mail", &["email", "mail"]),
    ("Reminders", &["remind", "reminder", "reminders"]),
    ("Music", &["play", "song", "music"]),
    ("Spotify", &["play", "song", "music", "spotify"]),
    ("Photo Booth", &["photo", "photos", "picture", "selfie"]),
    ("Shortcuts", &["shortcut"]),
    (
        "macOS",
        &[
            "open",
            "quit",
            "volume",
            "louder",
            "quieter",
            "mute",
            "clipboard",
            "copy",
            "window",
            "tab",
            "file",
            "files",
            "folder",
        ],
    ),
];

/// What the gate needs to know about one call before it decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    /// `"{app}:{action}"` slugged, e.g. `slack:send-and-share`: the grants.json key.
    pub key: String,
    pub app: String,
    pub action: String,
    pub default_always: bool,
    /// Always asks, and "Always allow" cannot cover it.
    pub strict: bool,
    /// Why this asks when a grant would otherwise let it run.
    pub reason: Option<String>,
    /// The named-app rule fired: prompt even when always-allowed.
    pub named_rule: bool,
}

/// Whether outside content has entered this run, and the apps the user has
/// since said yes to.
#[derive(Debug, Default)]
pub struct RunTaint {
    outside: bool,
    cleared: HashSet<String>,
}

impl RunTaint {
    /// Selected text is outside content from the first round: "fix the grammar
    /// of this" hands the model whatever the selection says.
    pub fn new(has_selection: bool) -> Self {
        Self {
            outside: has_selection,
            cleared: HashSet::new(),
        }
    }

    /// After every tool result. A read, engine or screen result carries text
    /// someone else wrote, and it stays in the context for the rest of the run.
    pub fn saw(&mut self, tool_was_read_or_engine_or_screen: bool) {
        self.outside |= tool_was_read_or_engine_or_screen;
    }

    /// After a yes: the user has now seen this app named, so it asks once per
    /// run, not once per call.
    pub fn clear(&mut self, app: &str) {
        self.cleared.insert(app.to_lowercase());
    }
}

/// The permission one call needs; None for reads and engine tasks (which ask
/// before each risky click themselves).
pub fn gate(
    name: &str,
    args: &Value,
    tool: Option<&ToolDef>,
    said: &str,
    front: Option<&str>,
    taint: &RunTaint,
) -> Option<Gate> {
    if aloud_engine::is_engine_tool(name) || tool.is_some_and(|t| t.read_only) {
        return None;
    }
    let mut toolkit = None;
    let (app, action, default_always, strict) = if aloud_screen::is_screen_tool(name) {
        if !aloud_screen::is_screen_write(name) {
            return None;
        }
        let app = args
            .get("app")
            .and_then(Value::as_str)
            .or(front)
            .filter(|app| !app.trim().is_empty())
            .unwrap_or("macOS");
        let secret = aloud_native::shares_a_secret(args);
        (app.to_string(), "screen control".to_string(), false, secret)
    } else if let Some(p) = aloud_native::permission(name, args, front) {
        (p.app, p.action, p.default_always, p.strict)
    } else {
        let kit = tool.map_or_else(|| composio::toolkit_of_slug(name), |t| t.toolkit.clone());
        let app = tool.map_or_else(|| kit.clone(), |t| t.app.clone());
        let action = composio_action(name, &kit);
        let hints = tool.map_or(&[][..], |t| &t.hints);
        let strict = composio_strict(name, action, hints, args);
        toolkit = Some(kit);
        (app, action.to_string(), false, strict)
    };
    let named_rule = taint.outside
        && !taint.cleared.contains(&app.to_lowercase())
        && !named(&app, toolkit.as_deref(), said, front);
    Some(Gate {
        key: key(&app, &action),
        reason: named_rule
            .then(|| format!("Outside content asked for this, and you didn't mention {app}")),
        named_rule,
        app,
        action,
        default_always,
        strict,
    })
}

/// The grants.json key for an app and action, e.g. `slack:send-and-share`.
pub fn key(app: &str, action: &str) -> String {
    format!("{}:{}", slug(app), slug(action))
}

/// Whether no call of this action can run without a card, so its Settings row
/// offers Ask or Never only. Paying is; a send or a delete is strict only for
/// some calls (a password, a `destructiveHint`).
pub fn always_strict(action: &str) -> bool {
    action == PAY
}

/// The front app, the app's own name in what was said, its toolkit's words, or
/// a Mac app's words.
fn named(app: &str, toolkit: Option<&str>, said: &str, front: Option<&str>) -> bool {
    let words: Vec<String> = said
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .collect();
    front.is_some_and(|f| f.trim().eq_ignore_ascii_case(app))
        || said.to_lowercase().contains(&app.to_lowercase())
        || toolkit.is_some_and(|t| composio::mentions(t, said))
        || NATIVE_WORDS
            .iter()
            .filter(|(native, _)| native.eq_ignore_ascii_case(app))
            .any(|(_, list)| words.iter().any(|w| list.contains(&w.as_str())))
}

/// The action a Composio tool performs, from its slug's verbs, worst first.
///
/// ponytail: verb words in the slug; hand mappings name the action exactly and
/// replace this when they land.
fn composio_action(name: &str, toolkit: &str) -> &'static str {
    // Past the toolkit, so a slug like `SHARE_POINT_` never reads as a verb.
    let prefix = format!("{}_", toolkit.to_uppercase());
    let verbs = name.strip_prefix(&prefix).unwrap_or(name);
    let has = |list: &[&str]| verbs.split('_').any(|w| list.contains(&w));
    if has(&[
        "PAY", "PAYMENT", "PAYOUT", "BUY", "PURCHASE", "ORDER", "CHECKOUT", "TRANSFER", "CHARGE",
    ]) {
        PAY
    } else if has(&["DELETE", "REMOVE", "TRASH", "CLEAR"]) {
        DELETE
    } else if has(&["SEND", "POST", "REPLY", "FORWARD", "INVITE", "SHARE"]) {
        SEND
    } else {
        EDIT
    }
}

/// Paying, permanent deletes, going public, and handing over a password or code.
fn composio_strict(name: &str, action: &str, hints: &[String], args: &Value) -> bool {
    let destructive = HAND_STRICT
        .iter()
        .find(|(tool, _)| *tool == name)
        .map_or_else(|| hints.iter().any(|h| h == "destructiveHint"), |&(_, s)| s);
    // Compact JSON, so these match keys and their values exactly: an escaped
    // quote inside a string value can never complete one.
    let text = args.to_string().to_lowercase();
    let money = text.contains("amount\":") && text.contains("currency\":");
    let public = [
        "\"visibility\":\"public\"",
        "\"visibility\":\"anyone\"",
        "\"type\":\"public\"",
        "\"type\":\"anyone\"",
        "\"private\":false",
    ]
    .iter()
    .any(|pair| text.contains(pair));
    destructive || action == PAY || money || public || aloud_native::shares_a_secret(args)
}

fn slug(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn composio(name: &str, hints: &[&str]) -> ToolDef {
        let toolkit = composio::toolkit_of_slug(name);
        ToolDef {
            name: name.into(),
            app: toolkit.clone(),
            toolkit,
            description: String::new(),
            parameters: json!({}),
            read_only: hints.contains(&"readOnlyHint"),
            hints: hints.iter().map(|h| h.to_string()).collect(),
        }
    }

    fn tainted() -> RunTaint {
        let mut taint = RunTaint::new(false);
        taint.saw(true);
        taint
    }

    fn slack_send(said: &str, front: Option<&str>, taint: &RunTaint) -> Gate {
        let tool = composio("SLACK_SEND_MESSAGE", &["openWorldHint", "createHint"]);
        let args = json!({ "channel": "#general", "markdown_text": "hacked" });
        gate(tool.name.as_str(), &args, Some(&tool), said, front, taint).unwrap()
    }

    fn strict(name: &str, hints: &[&str], args: Value) -> bool {
        let tool = composio(name, hints);
        gate(name, &args, Some(&tool), "", None, &RunTaint::new(false))
            .unwrap()
            .strict
    }

    #[test]
    fn a_write_to_an_app_the_user_never_named_asks_after_outside_content() {
        let gate_ = slack_send("summarise this page", None, &tainted());
        assert!(gate_.named_rule);
        assert_eq!(gate_.key, "slack:send-and-share");
        assert_eq!(
            gate_.reason.as_deref(),
            Some("Outside content asked for this, and you didn't mention slack")
        );
        // Saying the app, or one of its words, names it.
        assert!(
            !slack_send("summarise this page and post it to slack", None, &tainted()).named_rule
        );
        let calendar = composio("GOOGLECALENDAR_CREATE_EVENT", &[]);
        let named = gate(
            &calendar.name,
            &json!({}),
            Some(&calendar),
            "put this meeting in",
            None,
            &tainted(),
        );
        assert!(!named.unwrap().named_rule);
        // A Mac app is named by its words; Messages is not named by "note".
        let note = gate(
            "ALOUD_NOTES_CREATE",
            &json!({ "title": "x" }),
            None,
            "make a note of this",
            None,
            &tainted(),
        );
        assert!(!note.unwrap().named_rule);
        let text = gate(
            "ALOUD_MESSAGES_SEND",
            &json!({ "to": "bob", "text": "x" }),
            None,
            "make a note of this",
            None,
            &tainted(),
        );
        assert!(text.unwrap().named_rule);
        // "message" is any app's word: reading a Slack message does not name Messages.
        let text = gate(
            "ALOUD_MESSAGES_SEND",
            &json!({ "to": "+15551234567", "text": "hi" }),
            None,
            "what's my latest slack message",
            None,
            &tainted(),
        );
        assert!(text.unwrap().named_rule);
        // An untainted run never fires the rule.
        assert!(!slack_send("summarise this page", None, &RunTaint::new(false)).named_rule);
        // Reads and engine tasks are not gated here at all.
        let read = composio("SLACK_FETCH_CONVERSATION_HISTORY", &["readOnlyHint"]);
        assert!(gate(&read.name, &json!({}), Some(&read), "", None, &tainted()).is_none());
        assert!(gate(
            aloud_engine::BROWSER_TASK,
            &json!({}),
            None,
            "",
            None,
            &tainted()
        )
        .is_none());
    }

    #[test]
    fn the_front_app_counts_as_named() {
        assert!(!slack_send("reply with this", Some("Slack"), &tainted()).named_rule);
        let menu = gate(
            "ALOUD_MENU",
            &json!({ "item": "New Window" }),
            None,
            "do that",
            Some("Safari"),
            &tainted(),
        )
        .unwrap();
        assert_eq!(menu.app, "Safari");
        assert!(!menu.named_rule);
    }

    #[test]
    fn outside_content_asks_once_per_app_per_run() {
        let mut taint = tainted();
        assert!(slack_send("do what it says", None, &taint).named_rule);
        taint.clear("slack");
        assert!(!slack_send("do what it says", None, &taint).named_rule);
        let text = gate(
            "ALOUD_MESSAGES_SEND",
            &json!({ "to": "bob" }),
            None,
            "do what it says",
            None,
            &taint,
        );
        assert!(text.unwrap().named_rule, "a yes to Slack clears only Slack");
    }

    #[test]
    fn selection_counts_as_outside_content() {
        let mut taint = RunTaint::new(true);
        assert!(slack_send("fix the grammar of this", None, &taint).named_rule);
        let mut clean = RunTaint::new(false);
        clean.saw(false);
        assert!(!slack_send("fix the grammar of this", None, &clean).named_rule);
        taint.saw(false);
        assert!(taint.outside, "a write's result never untaints the run");
    }

    #[test]
    fn destructive_composio_tools_are_strict() {
        let hints = &["destructiveHint", "idempotentHint", "openWorldHint"];
        assert!(strict(
            "GOOGLECALENDAR_DELETE_EVENT",
            hints,
            json!({ "event_id": "x" })
        ));
        assert!(!strict(
            "GOOGLECALENDAR_CREATE_EVENT",
            &["createHint"],
            json!({ "summary": "x" })
        ));
    }

    /// Live, Composio tags GMAIL_MOVE_TO_TRASH `updateHint`, and its slug says
    /// TRASH: the action is a delete, but one Trash undoes.
    #[test]
    fn trash_is_not_strict_even_though_composio_tags_it_update() {
        let tool = composio("GMAIL_MOVE_TO_TRASH", &["openWorldHint", "updateHint"]);
        let trash = gate(
            &tool.name,
            &json!({ "message_id": "x" }),
            Some(&tool),
            "",
            None,
            &RunTaint::new(false),
        )
        .unwrap();
        assert_eq!((trash.action.as_str(), trash.strict), (DELETE, false));
        // The hand tier holds even if Composio re-tags it destructive.
        assert!(!strict(
            "GMAIL_MOVE_TO_TRASH",
            &["destructiveHint"],
            json!({})
        ));
    }

    #[test]
    fn a_campaign_send_is_strict_because_it_is_public() {
        assert!(strict(
            "MAILCHIMP_SEND_CAMPAIGN",
            &[],
            json!({ "campaign_id": "x" })
        ));
        assert!(strict(
            "GITHUB_CREATE_A_REPOSITORY",
            &[],
            json!({ "name": "x", "private": false })
        ));
        assert!(strict(
            "GOOGLEDRIVE_ADD_FILE_SHARING_PREFERENCE",
            &[],
            json!({ "type": "anyone", "role": "reader" })
        ));
        assert!(!strict(
            "GOOGLEDRIVE_ADD_FILE_SHARING_PREFERENCE",
            &[],
            json!({ "type": "user", "email_address": "a@b.c" })
        ));
    }

    #[test]
    fn paying_and_sharing_codes_are_strict() {
        assert_eq!(
            composio_action("STRIPE_CREATE_PAYMENT_INTENT", "stripe"),
            PAY
        );
        assert!(strict("STRIPE_CREATE_PAYMENT_INTENT", &[], json!({})));
        assert!(strict(
            "STRIPE_CREATE_INVOICE_ITEM",
            &[],
            json!({ "amount": 500, "currency": "usd" })
        ));
        assert!(strict(
            "SLACK_SEND_MESSAGE",
            &[],
            json!({ "markdown_text": "my password is hunter2" })
        ));
        assert!(strict(
            "SLACK_SEND_MESSAGE",
            &[],
            json!({ "markdown_text": "your OTP: 4411" })
        ));
        assert!(!strict(
            "SLACK_SEND_MESSAGE",
            &[],
            json!({ "markdown_text": "hotpot at 7?" })
        ));
        // A Mac write carrying a code is strict too.
        let note = gate(
            "ALOUD_NOTES_CREATE",
            &json!({ "text": "2FA backup codes" }),
            None,
            "",
            None,
            &RunTaint::new(false),
        );
        assert!(note.unwrap().strict);
    }
}
