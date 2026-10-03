//! Commands that can be told apart without a language model.
//!
//! A handful of phrasings carry most of the day — "go to youtube", "search lofi
//! on youtube" — and asking a model to classify them costs a round trip the
//! user can feel. They are matched here instead and everything else goes to the
//! brain. Conventions follow Home Assistant's hassil sentence templates
//! (Apache-2.0): fixed verbs, optional filler, a free-text slot, skip words
//! stripped first. The rules are conservative on purpose: a miss costs one
//! model call, a false match runs the wrong thing.

/// What a command is, when it can be told without a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fast {
    /// A website task: hand the whole utterance to the browser engine, which
    /// works out the start page itself (`startUrlFor` in `engine/`).
    Web,
}

/// Routes `text`, or `None` for the brain. An installed app's name outranks a
/// site of the same name, so "open notes" never becomes a web search.
pub fn route(text: &str, installed: &[aloud_selection::InstalledApp]) -> Option<Fast> {
    let names: Vec<String> = installed.iter().map(|a| a.name.to_lowercase()).collect();
    classify(text, &names)
}

/// Sites the engine recognises by name. ponytail: mirrors `SITES` in
/// `engine/lib/site.ts` by hand — the engine still picks the URL, this only
/// decides the route — so a site added there and not here just takes the
/// brain path.
const SITES: &[&str] = &[
    "wikipedia",
    "wiktionary",
    "mdn",
    "github",
    "hacker news",
    "npm",
    "google flights",
    "youtube",
    "reddit",
];

/// The engine's own address pattern's endings (`startUrlFor`), so a domain this
/// accepts is one the engine will open.
const TLDS: &[&str] = &[
    "com", "org", "net", "io", "dev", "ai", "co", "edu", "gov", "uk", "de", "in",
];

/// hassil's skip words: politeness that changes nothing.
const SKIP: &[&str] = &[
    "please ",
    "can you ",
    "could you ",
    "would you ",
    "will you ",
    "i want to ",
    "i'd like to ",
    "let's ",
];

/// Verbs whose object is where to go: the object must *start* with a site.
const NAVIGATE: &[&str] = &[
    "go to ",
    "open up ",
    "open ",
    "visit ",
    "navigate to ",
    "take me to ",
    "pull up ",
    "bring up ",
    "load ",
    "browse ",
];

/// Verbs whose object is what to look for: the site must be named with "on".
const SEARCH: &[&str] = &[
    "search for ",
    "search ",
    "look up ",
    "find ",
    "play ",
    "watch ",
    "check ",
    "show me ",
];

/// Phrasings that mean "search the web" with no site named.
const WEB_SEARCH: &[&str] = &[
    "google ",
    "search the web for ",
    "search the internet for ",
    "search online for ",
    "search google for ",
];

fn classify(text: &str, apps: &[String]) -> Option<Fast> {
    let said = normalize(text);

    if WEB_SEARCH
        .iter()
        .any(|p| said.starts_with(p) && said.len() > p.len())
    {
        return Some(Fast::Web);
    }

    if let Some(object) = after_verb(&said, NAVIGATE) {
        let object = object.strip_prefix("the ").unwrap_or(object);
        if starts_with_app(object, apps) {
            return None;
        }
        if starts_with_site(object) {
            return Some(Fast::Web);
        }
        return None;
    }

    if let Some(object) = after_verb(&said, SEARCH) {
        if names_a_site_with_on(object) {
            return Some(Fast::Web);
        }
    }
    None
}

/// Lowercase, single-spaced, no end punctuation, skip words gone.
fn normalize(text: &str) -> String {
    let mut said = text
        .trim()
        .trim_end_matches(['.', '!', '?', ','])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    while let Some(rest) = SKIP.iter().find_map(|s| said.strip_prefix(s)) {
        said = rest.to_string();
    }
    said.strip_suffix(" please").unwrap_or(&said).to_string()
}

fn after_verb<'a>(said: &'a str, verbs: &[&str]) -> Option<&'a str> {
    verbs.iter().find_map(|v| said.strip_prefix(v))
}

fn starts_with_app(object: &str, apps: &[String]) -> bool {
    apps.iter().any(|app| {
        !app.is_empty()
            && object
                .strip_prefix(app.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    })
}

fn starts_with_site(object: &str) -> bool {
    let first = object.split(' ').next().unwrap_or_default();
    is_domain(first)
        || SITES.iter().any(|site| {
            object
                .strip_prefix(site)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        })
}

/// "… on youtube", "… on adafruit.com", "… in wikipedia".
fn names_a_site_with_on(object: &str) -> bool {
    [" on ", " in ", " at "].iter().any(|marker| {
        object.split(marker).skip(1).any(|after| {
            let after = after.strip_prefix("the ").unwrap_or(after);
            starts_with_site(after)
        })
    })
}

/// "youtube.com", "https://x.dev/path" — the engine's pattern. An email address
/// is not a site: "email sam@gmail.com" must not open a browser.
fn is_domain(word: &str) -> bool {
    if word.contains('@') {
        return false;
    }
    let host = word
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default();
    let mut labels = host.split('.');
    let Some(tld) = host.rsplit('.').next() else {
        return false;
    };
    host.contains('.')
        && TLDS.contains(&tld)
        && labels.all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(text: &str) -> Option<Fast> {
        let apps = ["notes", "spotify", "slack", "safari", "calculator"].map(String::from);
        classify(text, &apps)
    }

    #[test]
    fn named_sites_and_addresses_go_straight_to_the_browser() {
        for text in [
            "Go to YouTube.",
            "open github.com",
            "Please open reddit",
            "open youtube and play lofi",
            "visit https://news.ycombinator.com/newest",
            "search lofi on youtube",
            "look up serendipity on wiktionary",
            "check the price of a raspberry pi on adafruit.com",
            "google best pizza near me",
            "search the web for tauri drag and drop",
        ] {
            assert_eq!(route(text), Some(Fast::Web), "{text}");
        }
    }

    /// Each of these once went somewhere it should not have, or would with a
    /// looser rule: an app, a local file, a connected account, a Mac task.
    #[test]
    fn apps_files_accounts_and_mac_tasks_are_left_to_the_brain() {
        for text in [
            "open notes",
            "Open the Notes app and create a new note",
            "open Cursor and create a file todo.md",
            "open the door",
            "play some jazz",
            "stop",
            "search for my tax pdf",
            "find my github username",
            "email sam@gmail.com the notes",
            "make a list of items to buy for a cake and add it to a note",
            "what's the capital of France",
        ] {
            assert_eq!(route(text), None, "{text}");
        }
    }

    #[test]
    fn an_installed_app_outranks_a_site_of_the_same_name() {
        let apps = ["youtube".to_string()];
        assert_eq!(classify("open youtube", &apps), None);
        assert_eq!(classify("search lofi on youtube", &apps), Some(Fast::Web));
    }

    #[test]
    fn domains_need_a_known_ending_and_no_at_sign() {
        assert!(is_domain("adafruit.com"));
        assert!(is_domain("https://x.dev/a/b"));
        assert!(!is_domain("resume.pdf"));
        assert!(!is_domain("sam@gmail.com"));
        assert!(!is_domain("notes"));
    }
}
