//! Commands that can be told apart without a language model.
//!
//! A handful of phrasings carry most of the day — "open Slack", "pause", "volume
//! 30", "go to youtube", "search lofi on youtube" — and asking a model to
//! classify them costs a round trip the user can feel. They are matched here
//! instead and everything else goes to the brain. Conventions follow Home
//! Assistant's hassil sentence templates (Apache-2.0): fixed verbs, optional
//! filler, a free-text slot, skip words stripped first. The rules are
//! conservative on purpose: a miss costs one model call, a false match runs the
//! wrong thing.

use serde_json::{json, Value};

/// What a command is, when it can be told without a model.
#[derive(Debug, Clone, PartialEq)]
pub enum Fast {
    /// A website task: hand the whole utterance to the browser engine, which
    /// works out the start page itself (`startUrlFor` in `engine/`), with the
    /// search terms when the command was a plain web search.
    Web { query: Option<String> },
    /// One existing native tool with these arguments, and what to say once it
    /// has run.
    Tool {
        name: &'static str,
        args: Value,
        done: String,
    },
}

/// Routes `text`, or `None` for the brain. An installed app's name outranks a
/// site of the same name, so "open notes" never becomes a web search.
pub fn route(text: &str, installed: &[aloud_selection::InstalledApp]) -> Option<Fast> {
    let names: Vec<String> = installed.iter().map(|a| a.name.to_lowercase()).collect();
    // Back to the app's own spelling, which is what opening it by path needs.
    let proper = |lower: &str| {
        installed
            .iter()
            .find(|a| a.name.to_lowercase() == lower)
            .map_or_else(|| lower.to_string(), |a| a.name.clone())
    };
    Some(match classify(text, &names)? {
        Intent::Web(query) => Fast::Web { query },
        Intent::Open(apps) => {
            let apps: Vec<String> = apps.iter().map(|a| proper(a)).collect();
            Fast::Tool {
                name: "ALOUD_OPEN_APP",
                done: format!("Opened {}.", apps.join(" and ")),
                args: json!({ "names": apps }),
            }
        }
        Intent::Quit(app) => {
            let app = proper(&app);
            Fast::Tool {
                name: "ALOUD_QUIT_APP",
                done: format!("Quit {app}."),
                args: json!({ "name": app }),
            }
        }
        Intent::Media(action) => {
            // Whichever player the user is actually using; Music ships with every Mac.
            let (name, app) = if spotify_running() {
                ("ALOUD_SPOTIFY", "Spotify")
            } else {
                ("ALOUD_MUSIC", "Music")
            };
            let done = match action {
                "play" => format!("Playing {app}."),
                "pause" => format!("Paused {app}."),
                "next" => format!("Skipped to the next track in {app}."),
                _ => format!("Went back a track in {app}."),
            };
            Fast::Tool {
                name,
                args: json!({ "action": action }),
                done,
            }
        }
        Intent::Volume(Volume::Level(level)) => Fast::Tool {
            name: "ALOUD_SYSTEM_VOLUME",
            args: json!({ "level": level }),
            done: if level == 0 {
                "Muted.".to_string()
            } else {
                format!("Volume at {level}%.")
            },
        },
        Intent::Volume(Volume::Change(change)) => Fast::Tool {
            name: "ALOUD_SYSTEM_VOLUME",
            args: json!({ "change": change }),
            done: if change > 0 {
                "Turned it up."
            } else {
                "Turned it down."
            }
            .to_string(),
        },
    })
}

fn spotify_running() -> bool {
    std::process::Command::new("/usr/bin/pgrep")
        .args(["-xq", "Spotify"])
        .status()
        .is_ok_and(|s| s.success())
}

/// What `classify` recognised, before it is mapped onto a tool.
#[derive(Debug, PartialEq)]
enum Intent {
    /// With the search terms, for "google best pizza".
    Web(Option<String>),
    /// Installed apps, lowercase.
    Open(Vec<String>),
    Quit(String),
    /// play, pause, next or previous.
    Media(&'static str),
    Volume(Volume),
}

#[derive(Debug, PartialEq)]
enum Volume {
    Level(i64),
    Change(i64),
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

/// Verbs that open or switch to apps: every named thing must be installed.
const OPEN_APP: &[&str] = &[
    "open up ",
    "open ",
    "launch ",
    "start ",
    "switch to ",
    "go to ",
    "bring up ",
    "pull up ",
];

const QUIT_APP: &[&str] = &["quit ", "close ", "exit "];

/// Whole commands only: "play some jazz" is a request for the brain, "play" is
/// the play button.
const MEDIA: &[(&str, &[&str])] = &[
    (
        "play",
        &[
            "play",
            "play music",
            "resume",
            "resume music",
            "resume the music",
            "unpause",
        ],
    ),
    (
        "pause",
        &[
            "pause",
            "pause it",
            "pause music",
            "pause the music",
            "stop the music",
            "stop music",
        ],
    ),
    (
        "next",
        &[
            "next",
            "next song",
            "next track",
            "skip",
            "skip it",
            "skip song",
            "skip this song",
            "skip track",
            "play the next song",
        ],
    ),
    (
        "previous",
        &[
            "previous",
            "previous song",
            "previous track",
            "last song",
            "play the previous song",
            "go back a song",
        ],
    ),
];

const LOUDER: &[&str] = &[
    "volume up",
    "turn it up",
    "turn the volume up",
    "turn up the volume",
    "louder",
    "raise the volume",
];
const QUIETER: &[&str] = &[
    "volume down",
    "turn it down",
    "turn the volume down",
    "turn down the volume",
    "quieter",
    "lower the volume",
];
const MUTE: &[&str] = &["mute", "mute it", "mute the volume", "mute the sound"];
/// Prefixes before a spoken level: "set the volume to 30 percent".
const VOLUME_TO: &[&str] = &[
    "set the volume to ",
    "set volume to ",
    "change the volume to ",
    "turn the volume to ",
    "volume to ",
    "volume ",
];
/// "Turn it up" by this many points: about two presses of the volume key.
const VOLUME_STEP: i64 = 10;

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

fn classify(text: &str, apps: &[String]) -> Option<Intent> {
    let said = normalize(text);

    if let Some((action, _)) = MEDIA
        .iter()
        .find(|(_, phrases)| phrases.contains(&said.as_str()))
    {
        return Some(Intent::Media(action));
    }
    if let Some(volume) = volume(&said) {
        return Some(Intent::Volume(volume));
    }
    if let Some(object) = after_verb(&said, QUIT_APP) {
        return only_app(object, apps).map(Intent::Quit);
    }
    // Every part must be an app: "open notes and create a new note" is a task
    // for the brain, and opening Notes alone would quietly drop half of it.
    if let Some(object) = after_verb(&said, OPEN_APP) {
        let opened: Option<Vec<String>> = object
            .split(" and ")
            .flat_map(|part| part.split(", "))
            .map(|part| only_app(part, apps))
            .collect();
        if let Some(opened) = opened.filter(|o| !o.is_empty()) {
            return Some(Intent::Open(opened));
        }
    }

    if let Some(query) = after_verb(&said, WEB_SEARCH).filter(|q| !q.trim().is_empty()) {
        return Some(Intent::Web(Some(query.trim().to_string())));
    }

    if let Some(object) = after_verb(&said, NAVIGATE) {
        let object = object.strip_prefix("the ").unwrap_or(object);
        if starts_with_app(object, apps) {
            return None;
        }
        if starts_with_site(object) {
            return Some(Intent::Web(None));
        }
        return None;
    }

    if let Some(object) = after_verb(&said, SEARCH) {
        if names_a_site_with_on(object) {
            return Some(Intent::Web(None));
        }
    }
    None
}

/// The installed app `phrase` names exactly ("the notes app" → notes), if any.
fn only_app(phrase: &str, apps: &[String]) -> Option<String> {
    let phrase = phrase.trim();
    let phrase = phrase.strip_prefix("the ").unwrap_or(phrase);
    let phrase = [" application", " apps", " app"]
        .iter()
        .find_map(|word| phrase.strip_suffix(word))
        .unwrap_or(phrase);
    apps.iter().find(|app| app.as_str() == phrase).cloned()
}

fn volume(said: &str) -> Option<Volume> {
    if MUTE.contains(&said) {
        return Some(Volume::Level(0));
    }
    if LOUDER.contains(&said) {
        return Some(Volume::Change(VOLUME_STEP));
    }
    if QUIETER.contains(&said) {
        return Some(Volume::Change(-VOLUME_STEP));
    }
    let level = after_verb(said, VOLUME_TO)?;
    let level = [" percent", "%"]
        .iter()
        .find_map(|unit| level.strip_suffix(unit))
        .unwrap_or(level);
    level
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|l| (0..=100).contains(l))
        .map(Volume::Level)
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

    fn route(text: &str) -> Option<Intent> {
        let apps = [
            "notes",
            "spotify",
            "slack",
            "safari",
            "calculator",
            "notion",
            "photo booth",
        ]
        .map(String::from);
        classify(text, &apps)
    }

    fn open(apps: &[&str]) -> Option<Intent> {
        Some(Intent::Open(apps.iter().map(|a| a.to_string()).collect()))
    }

    #[test]
    fn installed_apps_open_and_quit_without_a_model() {
        assert_eq!(route("Open the Notes app."), open(&["notes"]));
        assert_eq!(route("open calculator app"), open(&["calculator"]));
        assert_eq!(route("launch photo booth"), open(&["photo booth"]));
        assert_eq!(route("switch to Safari"), open(&["safari"]));
        assert_eq!(route("open Slack and Notion"), open(&["slack", "notion"]));
        assert_eq!(route("quit Safari"), Some(Intent::Quit("safari".into())));
        assert_eq!(route("close the window"), None);
    }

    #[test]
    fn bare_media_and_volume_commands_are_instant() {
        assert_eq!(route("Pause."), Some(Intent::Media("pause")));
        assert_eq!(route("next song"), Some(Intent::Media("next")));
        assert_eq!(route("resume the music"), Some(Intent::Media("play")));
        assert_eq!(
            route("set the volume to 30 percent"),
            Some(Intent::Volume(Volume::Level(30)))
        );
        assert_eq!(route("volume 70%"), Some(Intent::Volume(Volume::Level(70))));
        assert_eq!(route("mute"), Some(Intent::Volume(Volume::Level(0))));
        assert_eq!(
            route("turn it up"),
            Some(Intent::Volume(Volume::Change(VOLUME_STEP)))
        );
        assert_eq!(route("set the volume to 300"), None);
        assert_eq!(route("play my discover weekly on spotify"), None);
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
        ] {
            assert_eq!(route(text), Some(Intent::Web(None)), "{text}");
        }
        // A plain web search carries just its terms, not the "google" in front.
        assert_eq!(
            route("google best pizza near me"),
            Some(Intent::Web(Some("best pizza near me".into())))
        );
        assert_eq!(
            route("search the web for tauri drag and drop"),
            Some(Intent::Web(Some("tauri drag and drop".into())))
        );
    }

    /// Each of these once went somewhere it should not have, or would with a
    /// looser rule: half a task, a local file, a connected account, a Mac task.
    #[test]
    fn tasks_files_and_accounts_are_left_to_the_brain() {
        for text in [
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
        assert_eq!(classify("open youtube", &apps), open(&["youtube"]));
        assert_eq!(
            classify("search lofi on youtube", &apps),
            Some(Intent::Web(None))
        );
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
