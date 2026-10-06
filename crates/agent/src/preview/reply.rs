//! What the user gives back to a card: a spoken reply, edits typed into its
//! fields, and the countdown that answers for an exact-words message.

use super::hand::{name_of, prop_at, unescape};
use super::{ActionPreview, CardKind, Field, Origin, Role, Tier};
use serde_json::{Number, Value};
use std::collections::BTreeMap;

/// How long an exact-words message waits for an Undo before it sends.
pub const AUTO_MS: u32 = 3_000;

/// The countdown a card gets, if any: a message that is the user's own words,
/// to destinations a read named, on a card nothing made Strict, under a grant
/// that left it to the card. Anything the model wrote, or sent somewhere no read
/// named, waits for a click.
pub fn countdown(preview: &ActionPreview, grant_is_ask: bool) -> Option<u32> {
    let all = || preview.fields.iter().chain(&preview.more);
    let every = |of: fn(&Field) -> bool, ok: fn(&Field) -> bool| {
        let mut them = all().filter(|f| of(f)).peekable();
        them.peek().is_some() && them.all(ok)
    };
    let exact = every(
        |f| f.role == Role::Text,
        |f| f.origin == Some(Origin::Yours),
    );
    let named = every(
        |f| matches!(f.role, Role::Destination | Role::Person),
        |f| f.name.is_some(),
    );
    (preview.kind == CardKind::Message
        && preview.tier != Tier::Strict
        && grant_is_ask
        && exact
        && named)
        .then_some(AUTO_MS)
}

/// A spoken answer to the card on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateReply {
    Approve,
    Decline,
    /// The card's text, in the user's words.
    Replace(String),
}

// ponytail: fixed phrase lists, whole utterances only. A misheard command must
// not approve a card, so anything longer is a new command; widen from real
// misses in the timings, not by guessing.
const APPROVE: &[&str] = &[
    "send it",
    "send",
    "send it now",
    "yes",
    "yes please",
    "yes send it",
    "yeah",
    "yep",
    "ok",
    "okay",
    "do it",
    "go ahead",
    "confirm",
    "approve",
    "allow",
    "allow it",
];
const DECLINE: &[&str] = &[
    "cancel",
    "cancel it",
    "cancel that",
    "no",
    "nope",
    "no thanks",
    "never mind",
    "nevermind",
    "don't",
    "don't send it",
    "do not send it",
    "stop",
    "forget it",
];

/// What `said` answers, when it is a reply to the open card at all. Yes and no
/// match whole utterances only; a rewrite only in the anchored forms "make it
/// X", "change it to X" and "say X instead". Everything else is a new command.
pub fn gate_reply(said: &str) -> Option<GateReply> {
    let words = normalise(said);
    if APPROVE.contains(&words.as_str()) {
        return Some(GateReply::Approve);
    }
    if DECLINE.contains(&words.as_str()) {
        return Some(GateReply::Decline);
    }
    let said = said.trim();
    let rest = ["make it ", "change it to "]
        .iter()
        .find_map(|anchor| strip_prefix_ignoring_case(said, anchor))
        .or_else(|| {
            let quoted = strip_prefix_ignoring_case(said, "say ")?;
            let quoted = quoted.trim_end_matches(|c: char| c.is_ascii_punctuation());
            let cut = quoted.len().checked_sub(" instead".len())?;
            (quoted.is_char_boundary(cut) && quoted[cut..].eq_ignore_ascii_case(" instead"))
                .then(|| &quoted[..cut])
        })?;
    // Whisper ends every sentence with a full stop, which is not part of the
    // message; a question or exclamation mark is.
    let text = rest
        .trim()
        .trim_end_matches('.')
        .trim_matches(|c| matches!(c, '"' | '“' | '”' | '\''))
        .trim();
    (!text.is_empty()).then(|| GateReply::Replace(text.to_string()))
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

/// Lowercase words with the punctuation around them gone, apostrophes kept:
/// "Never mind." and "never mind" are one reply, "don't" is not "dont".
fn normalise(said: &str) -> String {
    said.to_lowercase()
        .replace('’', "'")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The call's arguments with the card's edits written in, each by the pointer
/// of the field it edits. Only a field the card offered as editable takes one;
/// a value is coerced to its schema type, and one outside the schema's `enum`
/// is refused. A refused edit refuses them all: the user approved the card
/// they saw, not part of it.
///
/// A missing parent is created — an empty address list gets its first entry —
/// and a new address clears the name `prepare` resolved the old one to.
pub fn apply_edits(
    args: &Value,
    edits: &BTreeMap<String, String>,
    preview: &ActionPreview,
    schema: Option<&Value>,
) -> Result<Value, String> {
    let mut out = args.clone();
    for (pointer, text) in edits {
        let offered = preview
            .fields
            .iter()
            .chain(&preview.more)
            .any(|f| f.editable && f.path == *pointer);
        if !offered {
            return Err(format!("{pointer} is not a field this card lets you edit"));
        }
        let value = coerce(text, prop_at(schema, pointer), out.pointer(pointer))
            .map_err(|why| format!("{pointer}: {why}"))?;
        put(&mut out, pointer, value)?;
        let name = name_of(pointer);
        if let Some((parent, key)) = name.rsplit_once('/') {
            if let Some(map) = out.pointer_mut(parent).and_then(Value::as_object_mut) {
                map.remove(&unescape(key));
            }
        }
    }
    Ok(out)
}

/// Writes the user's words over the card's text — its first editable text
/// field — and returns the arguments that now carry them. The card marks them
/// as the user's, and drops its countdown: words a transcriber just heard wait
/// for a yes. None when the card has no text to replace.
pub fn replace_text(
    preview: &mut ActionPreview,
    args: &Value,
    schema: Option<&Value>,
    text: &str,
) -> Option<Value> {
    let path = preview
        .fields
        .iter()
        .chain(&preview.more)
        .find(|f| f.editable && f.role == Role::Text)?
        .path
        .clone();
    let edits = BTreeMap::from([(path.clone(), text.to_string())]);
    let args = apply_edits(args, &edits, preview, schema).ok()?;
    if let Some(field) = preview
        .fields
        .iter_mut()
        .chain(preview.more.iter_mut())
        .find(|f| f.path == path)
    {
        field.value = text.to_string();
        field.origin = Some(Origin::Yours);
    }
    preview.auto_ms = None;
    Some(args)
}

/// The edited text as the value its field takes: the schema's type when it
/// has one, else the type the field already held.
fn coerce(text: &str, prop: Option<&Value>, was: Option<&Value>) -> Result<Value, String> {
    let value = match prop.and_then(|p| p.get("type")).and_then(Value::as_str) {
        Some("string") => Value::String(text.to_string()),
        Some("integer") => text
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| format!("{text:?} is not a whole number"))?,
        Some("number") => text
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| format!("{text:?} is not a number"))?,
        Some("boolean") => match text.trim() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(format!("{text:?} is not true or false")),
        },
        Some("array" | "object") => {
            serde_json::from_str(text).map_err(|_| format!("{text:?} is not a list"))?
        }
        // Untyped: a string stays one, and anything else parses back to the
        // shape it had. Text that no longer parses is kept rather than dropped
        // — a tool rejecting a bad argument says so, a missing one just does
        // the wrong thing.
        _ => match was {
            Some(Value::String(_)) | None => Value::String(text.to_string()),
            Some(_) => {
                serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
            }
        },
    };
    let allowed = prop.and_then(|p| p.get("enum")).and_then(Value::as_array);
    if allowed.is_some_and(|allowed| !allowed.contains(&value)) {
        return Err(format!("{value} is not one of its choices"));
    }
    Ok(value)
}

/// Sets `pointer` in `root`, creating each missing parent on the way: an array
/// where the next step is an index, else an object. An index may append to a
/// list but never leave a hole in it.
fn put(root: &mut Value, pointer: &str, value: Value) -> Result<(), String> {
    let segments: Vec<String> = pointer
        .strip_prefix('/')
        .ok_or_else(|| format!("{pointer:?} is not a pointer"))?
        .split('/')
        .map(unescape)
        .collect();
    let mut at = root;
    for (i, segment) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        let fresh = || match segments.get(i + 1) {
            Some(next) if next.parse::<usize>().is_ok() => Value::Array(Vec::new()),
            _ => Value::Object(Default::default()),
        };
        let slot = match at {
            Value::Object(map) => map.entry(segment.as_str()).or_insert(Value::Null),
            Value::Array(list) => {
                let index: usize = segment
                    .parse()
                    .map_err(|_| format!("{segment:?} in {pointer} is not an index"))?;
                if index == list.len() {
                    list.push(Value::Null);
                }
                list.get_mut(index)
                    .ok_or_else(|| format!("{pointer} skips past the end of its list"))?
            }
            _ => return Err(format!("{pointer} runs through a value that holds nothing")),
        };
        if last {
            *slot = value;
            return Ok(());
        }
        if slot.is_null() {
            *slot = fresh();
        }
        at = slot;
    }
    Err(format!("{pointer:?} names no field"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::hand::tests::schema_of;
    use crate::preview::{build, render, Context, Names};
    use serde_json::json;

    fn card(name: &str, args: &Value, said: &str, names: &Names) -> ActionPreview {
        render(
            name,
            args,
            schema_of(name).as_ref(),
            &Context { said, names },
        )
    }

    fn launch() -> Names {
        let mut names = Names::default();
        names.harvest(
            &json!({ "channels": [{ "id": "C0123ABCD45", "name": "launch" }] }),
            "SLACK_FIND_CHANNELS launch",
        );
        names
    }

    #[test]
    fn an_exact_message_to_a_named_channel_counts_down() {
        let names = launch();
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "standup moved to 10" });
        let p = card(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            "send standup moved to 10 to launch on slack",
            &names,
        );
        assert_eq!(countdown(&p, true), Some(AUTO_MS));
        assert_eq!(
            countdown(&p, false),
            None,
            "a saved Always shows no card, and outside content waits for a click"
        );

        // A text to someone `prepare` found in Contacts: the name is on the card.
        let p = card(
            "ALOUD_MESSAGES_SEND",
            &json!({ "to": "+15550001111", "to_name": "Suhani Shah", "text": "on my way" }),
            "text suhani on my way",
            &names,
        );
        assert_eq!(p.title, "Send message to Suhani Shah");
        assert_eq!(countdown(&p, true), Some(AUTO_MS));
    }

    /// Mail and FaceTime had no mapping, so their cards fell back to the
    /// schema and showed the number `prepare` found, not the person.
    #[test]
    fn mail_and_facetime_cards_name_the_contact_and_never_count_down() {
        let names = launch();
        let mail = card(
            "ALOUD_MAIL_SEND",
            &json!({ "to": "suhani@example.com", "to_name": "Suhani Shah", "subject": "Hi", "body": "on my way" }),
            "email suhani on my way",
            &names,
        );
        assert_eq!(mail.title, "Send email to Suhani Shah");
        assert_eq!(countdown(&mail, true), None, "only a message counts down");

        let call = card(
            "ALOUD_FACETIME_CALL",
            &json!({ "to": "+15550001111", "to_name": "Suhani Shah" }),
            "facetime suhani",
            &names,
        );
        assert_eq!(call.title, "Call Suhani Shah");
        assert_eq!(countdown(&call, true), None, "a call carries no words");
    }

    #[test]
    fn a_message_aloud_wrote_waits_for_a_click() {
        let names = launch();
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "Hi team — standup is moving to 10:00 today." });
        let p = card(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            "tell launch standup moved",
            &names,
        );
        assert_eq!(countdown(&p, true), None);

        // One text field the model filled in beside the user's own is enough.
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "standup moved", "text": "fallback the model wrote" });
        let p = card(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            "send standup moved to launch",
            &names,
        );
        assert_eq!(countdown(&p, true), None);
    }

    #[test]
    fn an_unresolved_destination_waits_for_a_click() {
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "standup moved" });
        let p = card(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            "send standup moved to launch",
            &Names::default(),
        );
        assert_eq!(countdown(&p, true), None);

        // A handle typed straight in names nobody.
        let p = card(
            "ALOUD_MESSAGES_SEND",
            &json!({ "to": "+15550001111", "text": "on my way" }),
            "text 555 0001111 on my way",
            &Names::default(),
        );
        assert_eq!(countdown(&p, true), None);
    }

    #[test]
    fn strict_never_counts_down() {
        let names = launch();
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "standup moved" });
        let mut p = card(
            "SLACK_CHAT_POST_MESSAGE",
            &args,
            "send standup moved to launch",
            &names,
        );
        p.tier = Tier::Strict;
        assert_eq!(countdown(&p, true), None);

        // Nor does anything that is not a message, however exact.
        let p = card(
            "ALOUD_NOTES_CREATE",
            &json!({ "title": "groceries", "body": "eggs" }),
            "create a note groceries eggs",
            &names,
        );
        assert_eq!(countdown(&p, true), None);
    }

    #[test]
    fn send_it_approves_and_cancel_declines() {
        for said in ["Send it.", "yes", "Yes, send it!", "OK", "go ahead"] {
            assert_eq!(gate_reply(said), Some(GateReply::Approve), "{said}");
        }
        for said in [
            "Cancel.",
            "never mind",
            "Nevermind!",
            "No.",
            "Don’t send it.",
        ] {
            assert_eq!(gate_reply(said), Some(GateReply::Decline), "{said}");
        }
    }

    #[test]
    fn make_it_replaces_the_text_with_the_users_words() {
        assert_eq!(
            gate_reply("Make it hey team."),
            Some(GateReply::Replace("hey team".into()))
        );
        assert_eq!(
            gate_reply("change it to Running late, sorry!"),
            Some(GateReply::Replace("Running late, sorry!".into()))
        );
        assert_eq!(
            gate_reply("Say “see you at 6” instead."),
            Some(GateReply::Replace("see you at 6".into()))
        );
        assert_eq!(gate_reply("make it"), None);

        // Written over the card's text and the call's: the card says they are
        // the user's words, and waits for a yes.
        let names = launch();
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "Hello everyone!" });
        let schema = schema_of("SLACK_CHAT_POST_MESSAGE");
        let mut p = card("SLACK_CHAT_POST_MESSAGE", &args, "say hi to launch", &names);
        p.auto_ms = Some(AUTO_MS);
        let sent = replace_text(&mut p, &args, schema.as_ref(), "hey team").expect("has text");
        assert_eq!(sent["markdown_text"], "hey team");
        assert_eq!(sent["channel"], "C0123ABCD45");
        let text = p
            .fields
            .iter()
            .find(|f| f.path == "/markdown_text")
            .unwrap();
        assert_eq!(
            (text.value.as_str(), text.origin),
            ("hey team", Some(Origin::Yours))
        );
        assert_eq!(p.auto_ms, None);

        // A card with no text to rewrite takes no rewrite.
        let mut p = card(
            "ALOUD_QUIT_APP",
            &json!({ "name": "Safari" }),
            "",
            &Names::default(),
        );
        assert_eq!(
            replace_text(&mut p, &json!({ "name": "Safari" }), None, "x"),
            None
        );
    }

    #[test]
    fn a_fresh_command_is_not_a_gate_reply() {
        for said in [
            "send hi to bob on slack",
            "yes and also text mom",
            "cancel my 3pm meeting",
            "make a note called groceries",
            "say hello to the team on slack",
            "stop listening",
            "",
        ] {
            assert_eq!(gate_reply(said), None, "{said}");
        }
    }

    /// A card offering exactly `paths` as editable, for edits with no mapping.
    fn offering(paths: &[&str]) -> ActionPreview {
        let mut preview = build("ANY_TOOL", &json!({}), None);
        preview.fields = paths
            .iter()
            .map(|path| Field {
                role: Role::Plain,
                path: path.to_string(),
                label: String::new(),
                value: String::new(),
                items: Vec::new(),
                count: 0,
                name: None,
                unresolved: false,
                before: None,
                origin: None,
                editable: true,
                cells: Vec::new(),
            })
            .collect();
        preview
    }

    fn edits(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn an_edit_to_a_field_the_card_did_not_offer_is_refused() {
        let args = json!({ "channel": "C0123ABCD45", "markdown_text": "hi" });
        let p = card("SLACK_CHAT_POST_MESSAGE", &args, "", &launch());
        let schema = schema_of("SLACK_CHAT_POST_MESSAGE");
        // The channel is on the card, but not to be changed there.
        let moved = edits(&[("/markdown_text", "hey"), ("/channel", "C0999ZZZZ99")]);
        assert!(apply_edits(&args, &moved, &p, schema.as_ref()).is_err());
        let kept = apply_edits(
            &args,
            &edits(&[("/markdown_text", "hey")]),
            &p,
            schema.as_ref(),
        );
        assert_eq!(kept.expect("offered")["markdown_text"], "hey");

        // A value outside the schema's choices is refused too.
        let schema = json!({ "properties": { "folder": { "type": "string", "enum": ["Desktop", "Downloads"] } } });
        let p = offering(&["/folder"]);
        let args = json!({ "folder": "Downloads" });
        assert!(apply_edits(&args, &edits(&[("/folder", "/etc")]), &p, Some(&schema)).is_err());
        assert!(apply_edits(&args, &edits(&[("/folder", "Desktop")]), &p, Some(&schema)).is_ok());
    }

    #[test]
    fn an_edit_to_an_empty_list_creates_its_first_entry() {
        // The contact the agent could not find an address for: the card shows
        // an empty Email field, and filling it writes the People API's shape.
        let args = json!({
            "names": [{ "givenName": "Dad" }],
            "emailAddresses": [],
            "personFields": "names,emailAddresses",
        });
        let name = "GOOGLECONTACTS_CREATE_CONTACT";
        let schema = schema_of(name);
        let p = card(name, &args, "", &Names::default());
        let email = p
            .fields
            .iter()
            .find(|f| f.path == "/emailAddresses/0/value")
            .expect("an empty address is still on the card");
        assert!(email.editable && email.value.is_empty());

        let filled = edits(&[("/emailAddresses/0/value", "dad@example.com")]);
        let sent = apply_edits(&args, &filled, &p, schema.as_ref()).expect("offered");
        assert_eq!(
            sent["emailAddresses"],
            json!([{ "value": "dad@example.com" }])
        );
        assert_eq!(sent["names"][0]["givenName"], "Dad");
        assert_eq!(sent["personFields"], "names,emailAddresses");

        // With no list at all, the list is made too.
        let sent = apply_edits(&json!({}), &filled, &p, schema.as_ref()).expect("offered");
        assert_eq!(sent["emailAddresses"][0]["value"], "dad@example.com");
    }

    #[test]
    fn a_new_address_is_no_longer_the_person_contacts_resolved() {
        let args = json!({ "to": "+15550001111", "to_name": "Suhani Shah", "text": "hi" });
        let p = card("ALOUD_MESSAGES_SEND", &args, "", &Names::default());
        let schema = schema_of("ALOUD_MESSAGES_SEND");
        let sent = apply_edits(
            &args,
            &edits(&[("/to", "+15559990000")]),
            &p,
            schema.as_ref(),
        )
        .expect("offered");
        assert_eq!(sent, json!({ "to": "+15559990000", "text": "hi" }));
    }

    /// Migrated from agent.rs: edits rebuild the shape the tool expects.
    /// Getting this wrong writes a field as a quoted JSON string, or drops it —
    /// both of which look like a successful save.
    #[test]
    fn edits_fold_back_into_the_shape_the_tool_expects() {
        let args = json!({
            "names": [{ "givenName": "Dad" }],
            "emailAddresses": [{ "value": "old@example.com" }],
            "personFields": "names,emailAddresses",
            "count": 2,
        });
        let p = offering(&["/emailAddresses/0/value", "/personFields", "/count"]);
        let sent = apply_edits(
            &args,
            &edits(&[
                ("/emailAddresses/0/value", "dad@example.com"),
                ("/personFields", "names,emailAddresses"),
                ("/count", "3"),
            ]),
            &p,
            None,
        )
        .expect("offered");
        assert_eq!(sent["emailAddresses"][0]["value"], "dad@example.com");
        // A string stays a string; a number parses back to one.
        assert_eq!(sent["personFields"], "names,emailAddresses");
        assert_eq!(sent["count"], 3);
        assert_eq!(sent["names"][0]["givenName"], "Dad");

        // A schema says what a value is, whatever it looks like.
        let schema = json!({ "properties": { "count": { "type": "string" } } });
        let sent = apply_edits(&args, &edits(&[("/count", "3")]), &p, Some(&schema));
        assert_eq!(sent.expect("offered")["count"], "3");
    }

    #[test]
    fn an_edit_that_stops_being_json_is_kept_not_dropped() {
        let args = json!({ "emailAddresses": [{ "value": "a@b.c" }] });
        let p = offering(&["/emailAddresses"]);
        let sent = apply_edits(&args, &edits(&[("/emailAddresses", "not json")]), &p, None);
        assert_eq!(sent.expect("offered")["emailAddresses"], "not json");
    }

    #[test]
    fn a_field_the_tool_never_had_arrives_as_a_string() {
        let p = offering(&["/subject"]);
        let sent = apply_edits(&json!({}), &edits(&[("/subject", "Hello")]), &p, None);
        assert_eq!(sent.expect("offered")["subject"], "Hello");
    }
}
