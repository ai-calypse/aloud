//! Who "Suhani" is: a Contacts lookup, and the pure rule for when one result is
//! certain enough to message without asking.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::script::{self, ScriptError};

/// Enough to offer as options, few enough to read out.
const MAX: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Person {
    pub name: String,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
}

/// The people whose full name, first name, last name or nickname contains
/// `query`, case-insensitively (Contacts' own `whose` comparison), at most 8.
///
/// ponytail: JXA against Contacts.app, so no dependency and no Info.plist key,
/// but 1-5s when Contacts is not running and 150-300ms when it is. objc2-contacts
/// (CNContactStore) is the upgrade if that cold start bites: no app at all.
///
/// The first Apple Event launches Contacts if it is closed. It comes up hidden
/// and not frontmost (lsappinfo reports it hidden), with only a Dock icon. It is
/// left running, because quitting it would make every lookup pay the cold start.
pub fn find(query: &str) -> Result<Vec<Person>, ScriptError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    // `whose` filters inside Contacts, so only the matches cross the bridge.
    let out = script::run_jxa(
        "Contacts",
        r#"const C = Application("Contacts");
        const q = ARG.query;
        const hits = C.people.whose({ _or: [
            { name: { _contains: q } }, { firstName: { _contains: q } },
            { lastName: { _contains: q } }, { nickname: { _contains: q } },
        ] })();
        return JSON.stringify(hits.slice(0, ARG.max).map(p => ({
            name: p.name() || "", phones: p.phones.value(), emails: p.emails.value(),
        })));"#,
        &json!({ "query": query, "max": MAX }),
    )?;
    serde_json::from_str(&out)
        .map_err(|e| ScriptError::Failed(format!("reading Contacts' answer: {e}")))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Match {
    One(Person),
    Many(Vec<Person>),
    None,
}

/// Picks a person only when the choice is not a guess: a unique exact full
/// name, else a unique exact first name, else the sole result. Two contacts who
/// both answer to "Suhani" come back as `Many` so the user is asked.
pub fn rank(query: &str, mut people: Vec<Person>) -> Match {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let query = norm(query);
    // ponytail: the first word of the display name stands in for the first
    // name, which is wrong for last-name-first locales; carry `firstName`
    // through `Person` if that ever matters.
    let first = |p: &Person| p.name.split_whitespace().next().map(str::to_lowercase);
    let picked = only(&mut people, |p| norm(&p.name) == query)
        .or_else(|| only(&mut people, |p| first(p).as_deref() == Some(query.as_str())));
    match (picked, people.len()) {
        (Some(person), _) => Match::One(person),
        (None, 0) => Match::None,
        (None, 1) => Match::One(people.remove(0)),
        (None, _) => Match::Many(people),
    }
}

/// Removes and returns the one person `hit` accepts, if exactly one does.
fn only(people: &mut Vec<Person>, hit: impl Fn(&Person) -> bool) -> Option<Person> {
    let mut hits = people.iter().enumerate().filter(|(_, p)| hit(p));
    match (hits.next().map(|(i, _)| i), hits.next()) {
        (Some(i), None) => Some(people.swap_remove(i)),
        _ => None,
    }
}

/// Already an address — an email, or a phone number of at least 7 digits — so
/// there is nobody to look up.
pub fn looks_like_handle(s: &str) -> bool {
    let s = s.trim();
    if let Some((user, domain)) = s.split_once('@') {
        return !user.is_empty() && domain.contains('.') && !s.contains(char::is_whitespace);
    }
    s.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c))
        && s.chars().filter(char::is_ascii_digit).count() >= 7
}

/// Turns the person named in `to` into the handle Messages, Mail or FaceTime
/// needs, before the gate, so the user confirms the person rather than a guess
/// they never saw. One match sets `to` to a handle and `to_name` to who it is;
/// several or none is an `Err` meant as the tool result, so the model asks.
/// A handle passes through untouched. None for every other tool.
///
/// Blocking: the lookup is an osascript run.
pub fn prepare(name: &str, args: &Value) -> Option<Result<Value, Value>> {
    if !matches!(
        name,
        "ALOUD_MESSAGES_SEND" | "ALOUD_MAIL_SEND" | "ALOUD_FACETIME_CALL"
    ) {
        return None;
    }
    let to = args.get("to").and_then(Value::as_str).map_or("", str::trim);
    // Empty is the tool's own "missing argument" to report.
    if to.is_empty() || looks_like_handle(to) {
        return Some(Ok(args.clone()));
    }
    Some(
        find(to)
            .map_err(|e| json!({ "error": e.to_string() }))
            .and_then(|people| resolve(name, args, rank(to, people), chat_handles)),
    )
}

/// What [`prepare`] makes of a match. `chats` is asked only for a Messages
/// send to someone with more than one handle, the one case it can change.
fn resolve(
    name: &str,
    args: &Value,
    found: Match,
    chats: impl FnOnce() -> Vec<String>,
) -> Result<Value, Value> {
    let to = args["to"].as_str().unwrap_or_default().trim();
    let person = match found {
        Match::One(person) => person,
        // Names only: the numbers of people the user did not mean stay out of
        // the transcript and the model's context.
        Match::Many(people) => {
            return Err(json!({
                "error": "several contacts match",
                "candidates": people.into_iter().map(|p| p.name).collect::<Vec<_>>(),
                "note": "ask which one, with these names as options",
            }))
        }
        Match::None => return Err(json!({ "error": format!("no contact named {to}") })),
    };
    // Messages and FaceTime both reach an Apple ID email when there is no phone.
    let handles = if name == "ALOUD_MAIL_SEND" {
        person.emails
    } else {
        [person.phones, person.emails].concat()
    };
    let in_chats = if name == "ALOUD_MESSAGES_SEND" && handles.len() > 1 {
        chats()
    } else {
        Vec::new()
    };
    let Some(handle) = best_handle(&handles, &in_chats) else {
        let what = if name == "ALOUD_MAIL_SEND" {
            "email address"
        } else {
            "phone number or email"
        };
        return Err(json!({ "error": format!("{} has no {what} in Contacts", person.name) }));
    };
    let mut args = args.clone();
    args["to"] = json!(handle);
    args["to_name"] = json!(person.name);
    Ok(args)
}

/// The handle already used in a Messages chat, else the first: the number the
/// conversation lives on beats a work line nobody texts.
fn best_handle<'a>(handles: &'a [String], in_chats: &[String]) -> Option<&'a String> {
    handles
        .iter()
        .find(|h| in_chats.iter().any(|c| same_handle(h, c)))
        .or(handles.first())
}

/// Contacts stores "+91 98765 43210", Messages "+919876543210", and a local
/// number drops the country code: phones compare on their last 10 digits.
fn same_handle(a: &str, b: &str) -> bool {
    if a.contains('@') || b.contains('@') {
        return a.trim().eq_ignore_ascii_case(b.trim());
    }
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let (a, b) = (digits(a), digits(b));
    let n = a.len().min(b.len()).min(10);
    n >= 7 && a[a.len() - n..] == b[b.len() - n..]
}

/// Every handle in the user's Messages chats, one Apple Event for the lot. A
/// failure (no grant, Messages broken) is only a lost preference, so it is
/// empty rather than an error.
fn chat_handles() -> Vec<String> {
    script::run_jxa(
        "Messages",
        r#"return JSON.stringify([].concat(...Application("Messages").chats.participants.handle()));"#,
        &json!({}),
    )
    .ok()
    .and_then(|out| serde_json::from_str(&out).ok())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str) -> Person {
        Person {
            name: name.into(),
            phones: vec![],
            emails: vec![],
        }
    }

    #[test]
    fn a_first_name_shared_by_two_contacts_asks_instead_of_picking() {
        let both = vec![person("Suhani Shah"), person("Suhani Mehta")];
        assert_eq!(rank("suhani", both.clone()), Match::Many(both));
        // A longer name that merely contains it does not share the first name.
        assert_eq!(
            rank(
                "Suhani",
                vec![person("Suhanika Rao"), person("Suhani Shah")]
            ),
            Match::One(person("Suhani Shah"))
        );
        assert_eq!(rank("Suhani", vec![]), Match::None);
    }

    #[test]
    fn an_exact_full_name_wins_over_partial_matches() {
        let people = vec![
            person("Suhani Shah-Kapoor"),
            person("Suhani Shah"),
            person("Suhani Shah Mehta"),
        ];
        assert_eq!(
            rank("  suhani   SHAH ", people),
            Match::One(person("Suhani Shah"))
        );
    }

    #[test]
    fn a_handle_is_never_looked_up_again() {
        for handle in [
            "suhani@example.com",
            "+91 98765 43210",
            "(415) 555-0132",
            "415-555-0132",
            "4155550132",
        ] {
            assert!(looks_like_handle(handle), "{handle} is a handle");
        }
        for name in ["Suhani", "mom", "Suhani Shah", "555-01", "@home", "a@b"] {
            assert!(!looks_like_handle(name), "{name} is not a handle");
        }
        // Through `prepare` unchanged: no Contacts lookup, no `to_name`.
        let args = json!({ "to": "+91 98765 43210", "text": "hi" });
        assert_eq!(
            prepare("ALOUD_MESSAGES_SEND", &args),
            Some(Ok(args.clone()))
        );
        assert_eq!(prepare("ALOUD_NOTES_CREATE", &args), None);
    }

    #[test]
    fn an_existing_chat_handle_wins_over_another_number() {
        let handles = ["+1 (415) 555-0132".to_string(), "+91 98765 43210".into()];
        let chats = ["+919876543210".to_string(), "someone@example.com".into()];
        assert_eq!(best_handle(&handles, &chats), Some(&handles[1]));
        // No chat with either: the first, as Contacts lists them.
        assert_eq!(best_handle(&handles, &[]), Some(&handles[0]));
        assert_eq!(best_handle(&[], &chats), None);
        // A local number is the same line as its international form.
        assert!(same_handle("098765 43210", "+919876543210"));
        assert!(!same_handle("555-0132", "+14155550199"));
    }

    #[test]
    fn several_matching_contacts_come_back_as_names_only() {
        let with_handles = |name: &str, phone: &str| Person {
            name: name.into(),
            phones: vec![phone.into()],
            emails: vec![format!("{phone}@example.com")],
        };
        let args = json!({ "to": "Suhani", "text": "hi" });
        let found = Match::Many(vec![
            with_handles("Suhani Shah", "+15550001111"),
            with_handles("Suhani Mehta", "+15550002222"),
        ]);
        let err = resolve("ALOUD_MESSAGES_SEND", &args, found, Vec::new).unwrap_err();
        assert_eq!(err["candidates"], json!(["Suhani Shah", "Suhani Mehta"]));
        assert!(!err.to_string().contains("555"), "a handle leaked: {err}");

        let none = resolve("ALOUD_MAIL_SEND", &args, Match::None, Vec::new).unwrap_err();
        assert_eq!(none["error"], "no contact named Suhani");

        // One person: the phone for Messages, the email for Mail, and who it is.
        let one = || Match::One(with_handles("Suhani Shah", "+15550001111"));
        let sent = resolve("ALOUD_MESSAGES_SEND", &args, one(), Vec::new).unwrap();
        assert_eq!(sent["to"], "+15550001111");
        assert_eq!(sent["to_name"], "Suhani Shah");
        assert_eq!(sent["text"], "hi");
        let mailed = resolve("ALOUD_MAIL_SEND", &args, one(), Vec::new).unwrap();
        assert_eq!(mailed["to"], "+15550001111@example.com");
    }
}
