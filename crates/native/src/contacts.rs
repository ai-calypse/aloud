//! Who "Suhani" is: a Contacts lookup, and the pure rule for when one result is
//! certain enough to message without asking.

use serde::Deserialize;
use serde_json::json;

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
    }
}
