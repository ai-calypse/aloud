//! Native macOS actions as agent tools — the local half of agent mode.
//!
//! Composio covers anything with a cloud API; this covers what does not — play
//! music, add a reminder, reveal a file. Same tool shape the loop already
//! consumes, so it plugs into the existing gate, dedup, and budgets with one
//! dispatch branch and no new architecture. See `freeflow-agent-mode-mvp-plan`.
//!
//! macOS-only in substance; every other platform gets an empty tool list, the
//! way `notch.rs` and `aloud_volume` do.

mod catalogue;
pub mod contacts;
pub mod fast;
mod keys;
mod media;
mod notes;
mod organize;
mod reminders;
mod script;
mod search;
mod shortcuts;
mod thumbnail;

use serde_json::{json, Value};

pub use catalogue::{editor_project, show};
pub use script::ScriptError;

/// `args` with what the user was looking at when they spoke, for a native tool
/// to fall back on: `_front_app` (MENU, PRESS_KEYS, WINDOW_ARRANGE with no
/// `app`), `_front_document` and `_front_project` (OPEN_APP's bare names). The
/// caller merges them after the gate, so neither the model nor the confirmation
/// ever sees them.
///
/// ponytail: underscored keys riding in the model's arguments; a `Context`
/// parameter on `Action::run` is the upgrade.
pub fn with_front(
    args: &Value,
    app: Option<&str>,
    document: Option<&str>,
    project: Option<&str>,
) -> Value {
    let mut args = args.clone();
    if let Some(map) = args.as_object_mut() {
        let front = [
            ("_front_app", app),
            ("_front_document", document),
            ("_front_project", project),
        ];
        for (key, value) in front {
            if let Some(value) = value {
                map.insert(key.to_string(), json!(value));
            }
        }
    }
    args
}

/// What a finished write's Show button brings back, as (app, target) for
/// [`show`]: the note it made, the file it saved, the thing it opened. The tools
/// work in the background, so this is how the user goes and looks.
pub fn show_target(name: &str, args: &Value, result: &Value) -> Option<(String, String)> {
    let got = |key: &str| {
        result
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match name {
        "ALOUD_NOTES_CREATE" | "ALOUD_NOTES_APPEND" => Some(("Notes".into(), got("id")?)),
        "ALOUD_WRITE_FILE" => {
            let app = match args.get("open_with").and_then(Value::as_str) {
                Some("cursor") => "Cursor",
                Some("vscode") => "Visual Studio Code",
                _ => "Finder",
            };
            Some((app.into(), got("wrote")?))
        }
        "ALOUD_OPEN_APP" => {
            let app = result["opened"].as_array()?.last()?.as_str()?;
            Some((app.into(), got("target")?))
        }
        _ => None,
    }
}

/// A file preview for the notch grid — a base64 `data:` URI, or None when Quick
/// Look has no preview. The command layer maps search paths through this.
pub fn thumbnail(path: &str) -> Option<String> {
    thumbnail::data_uri(path)
}

/// The single tool that fronts the whole Shortcuts library. Enumerated, not one
/// tool per shortcut, and absent entirely when the library is empty — which is
/// most machines.
const RUN_SHORTCUT: &str = "ALOUD_RUN_SHORTCUT";

/// The most times one call runs a repeatable tool, so a misheard "two hundred"
/// is a short burst rather than minutes of shutter.
const MAX_TIMES: u32 = 20;

/// A native tool offered to the model. Deliberately *not* `aloud_agent::ToolDef`:
/// `aloud_agent` depends on this crate to dispatch, so depending back would
/// cycle. The caller maps this into `ToolDef` at the concat site — a three-field
/// copy, cheaper than a shared crate for two structs.
#[derive(Debug, Clone)]
pub struct NativeTool {
    pub name: String,
    pub description: String,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
    pub read_only: bool,
}

/// The native tools available on this machine, filtered to apps that are
/// actually installed — the local mirror of Composio's connected-toolkits idea.
/// No Music on this Mac, no Music tools; keeps the schema token count down too.
///
/// Off macOS this is always empty.
pub fn tools(installed: &[aloud_selection::InstalledApp]) -> Vec<NativeTool> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = installed;
        return Vec::new();
    }
    #[cfg(target_os = "macos")]
    {
        let mut tools: Vec<NativeTool> = catalogue::ACTIONS
            .iter()
            .filter(|action| app_installed(action.app, installed))
            .map(|action| {
                let mut parameters = (action.params)();
                if action.repeatable {
                    parameters["properties"]["times"] = json!({
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_TIMES,
                        "description": "How many times to do it; default 1"
                    });
                }
                NativeTool {
                    name: action.name.to_string(),
                    description: action.description.to_string(),
                    parameters,
                    read_only: action.read_only,
                }
            })
            .collect();

        // One tool for the whole Shortcuts library, and only when it is
        // non-empty — an empty `shortcuts list` must yield zero tools, not one
        // that can only fail. The available names go in the schema so the model
        // picks a real one.
        let library = shortcuts::list();
        if !library.is_empty() {
            tools.push(NativeTool {
                name: RUN_SHORTCUT.to_string(),
                description: format!(
                    "Run one of the user's Shortcuts by name. Available: {}.",
                    library.join(", ")
                ),
                parameters: json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "enum": library }
                    }
                }),
                // A shortcut can do anything; it must pass the gate.
                read_only: false,
            });
        }

        tools
    }
}

/// Runs a native tool by name. Returns `None` when `name` is not one of ours,
/// which is the signal for the dispatch branch to fall through to Composio —
/// so the `ALOUD_` prefix is never hardcoded at the call site.
///
/// The `Ok` value is a JSON result in the same shape Composio returns, so the
/// loop's `tool_succeeded` and the panel need no special case. A `ScriptError`
/// is surfaced as an error result the model can read back.
/// Writes too small to confirm: each is undone by saying the opposite (open/quit, play/pause, volume,
/// a window moved, a reminder or a note line added, the clipboard set). Asking "are you sure?" before
/// opening Safari made every command a two-step chore. Sending, quitting, Shortcuts and file moves or
/// writes are left out, so they still ask.
pub fn low_risk(name: &str, args: &Value) -> bool {
    if name == "ALOUD_WRITE_FILE" {
        return catalogue::creates_new_file(args);
    }
    if name == "ALOUD_PRESS_KEYS" {
        let keys = args.get("keys").and_then(Value::as_str).unwrap_or_default();
        return keys::parse(keys).is_ok_and(|chord| !keys::destructive(&chord));
    }
    if name == "ALOUD_MENU" {
        // Listing changes nothing; a click is as safe as the command it names.
        let item = args
            .get("item")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                args.get("path")
                    .and_then(Value::as_array)
                    .and_then(|p| p.last())
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default()
            .to_lowercase();
        const RISKY: &[&str] = &[
            "quit",
            "close",
            "delete",
            "trash",
            "log out",
            "shut down",
            "restart",
            "erase",
            "remove",
            "discard",
            "revert",
            "force",
        ];
        return !RISKY.iter().any(|word| item.contains(word));
    }
    matches!(
        name,
        "ALOUD_OPEN_APP"
            | "ALOUD_MUSIC"
            | "ALOUD_SPOTIFY"
            | "ALOUD_PHOTO_BOOTH_TAKE_PHOTO"
            | "ALOUD_REMINDERS_ADD"
            | "ALOUD_NOTES_APPEND"
            | "ALOUD_NOTES_CREATE"
            | "ALOUD_WINDOW_ARRANGE"
            | "ALOUD_BROWSER_TAB"
            | "ALOUD_CLIPBOARD"
            | "ALOUD_FINDER_REVEAL"
            | "ALOUD_SYSTEM_VOLUME"
    )
}

/// One sentence saying what a native write did, built from its arguments and
/// result — so a run that only did this needs no second model call to say so.
/// None for tools whose outcome the model should phrase (reads, searches, file
/// moves) or when the result lacks what the sentence needs.
pub fn confirmation(name: &str, args: &Value, result: &Value) -> Option<String> {
    let arg = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let got = |key: &str| {
        result
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let player = |app: &str| match arg("action")? {
        "play" => Some(match got("track") {
            Some(track) => format!("Playing {track}."),
            None => format!("Opened {app}'s search for {}.", arg("query")?),
        }),
        "pause" => Some(format!("Paused {app}.")),
        "next" | "previous" => Some(match got("track") {
            Some(track) => format!("Now playing {track}."),
            None => format!("Changed track in {app}."),
        }),
        _ => None,
    };
    // A repeat's result is `{done, times, last}`, and only a whole run gets here.
    if let Some(n) = result.get("times").and_then(Value::as_u64) {
        return Some(match name {
            "ALOUD_PHOTO_BOOTH_TAKE_PHOTO" => format!("Took {n} photos."),
            "ALOUD_MUSIC" | "ALOUD_SPOTIFY" if arg("action") == Some("previous") => {
                format!("Went back {n} tracks.")
            }
            "ALOUD_MUSIC" | "ALOUD_SPOTIFY" => format!("Skipped {n} tracks."),
            "ALOUD_PRESS_KEYS" => format!("Pressed {} {n} times.", arg("keys")?),
            "ALOUD_MENU" => format!(
                "Chose {} {n} times.",
                result.pointer("/last/clicked")?.as_str()?
            ),
            _ => return None,
        });
    }
    Some(match name {
        "ALOUD_NOTES_CREATE" => format!("Created the note “{}”.", got("title").or(arg("title"))?),
        "ALOUD_NOTES_APPEND" if result.get("created").is_some() => {
            format!("Created the note “{}”.", got("title")?)
        }
        "ALOUD_NOTES_APPEND" => format!("Added to “{}”.", got("title")?),
        "ALOUD_REMINDERS_ADD" => match arg("due") {
            Some(due) => format!(
                "Added a reminder: {}, due {}.",
                arg("text")?,
                due.replace('T', " ")
            ),
            None => format!("Added a reminder: {}.", arg("text")?),
        },
        "ALOUD_MUSIC" => player("Music")?,
        "ALOUD_SPOTIFY" => player("Spotify")?,
        "ALOUD_SYSTEM_VOLUME" => format!("Volume at {}%.", result.get("volume")?),
        "ALOUD_WINDOW_ARRANGE" => format!("Moved the window {}.", arg("position")?),
        "ALOUD_CLIPBOARD" if arg("action") == Some("write") => "Copied to the clipboard.".into(),
        "ALOUD_PHOTO_BOOTH_TAKE_PHOTO" => "Took a photo in Photo Booth.".into(),
        "ALOUD_BROWSER_TAB" if arg("action") == Some("open") => format!("Opened {}.", arg("url")?),
        "ALOUD_WRITE_FILE" => format!("Saved {}.", got("wrote")?),
        "ALOUD_PRESS_KEYS" => format!("Pressed {}.", arg("keys")?),
        "ALOUD_MENU" if result.get("clicked").is_some() => format!("Chose {}.", got("clicked")?),
        // Only with `open`: a bare open is so often the first step of a screen
        // task that finishing the run on it would stop before the task.
        "ALOUD_OPEN_APP"
            if result["not_installed"]
                .as_array()
                .is_some_and(Vec::is_empty) =>
        {
            format!(
                "Opened {} in {}.",
                arg("open")?,
                result["opened"].as_array()?.last()?.as_str()?
            )
        }
        _ => return None,
    })
}

pub fn execute(name: &str, args: &Value) -> Option<Result<Value, ScriptError>> {
    if name == RUN_SHORTCUT {
        tracing::info!(tool = name, "running native tool");
        let shortcut = args.get("name").and_then(Value::as_str).unwrap_or_default();
        if shortcut.is_empty() {
            return Some(Err(ScriptError::Failed("missing shortcut name".into())));
        }
        return Some(match shortcuts::run(shortcut) {
            Ok(out) => Ok(json!({ "ran": shortcut, "output": out })),
            Err(e) => Err(ScriptError::Failed(e)),
        });
    }
    let action = catalogue::ACTIONS.iter().find(|a| a.name == name)?;
    tracing::info!(tool = name, "running native tool");
    let times = times(action, args);
    if times == 1 {
        return Some((action.run)(args));
    }
    Some(match repeat(times, || (action.run)(args)) {
        Ok(last) => Ok(json!({ "done": times, "times": times, "last": last })),
        // Nothing ran yet: the error as it came, so a permission denial still reads as one.
        Err((0, e)) => Err(e),
        // An Err, not a result with an `error` field, so no caller can read it as done.
        Err((done, e)) => Err(ScriptError::Failed(format!(
            "stopped after {done} of {times}: {e}"
        ))),
    })
}

/// How many times to run `action`: the model's `times` clamped to 1..=MAX_TIMES.
/// Once for a tool that is not repeatable, and for a player action other than
/// next/previous — the only repeatable tools with an `action` — since a second
/// pause changes nothing and a second play restarts the song.
fn times(action: &catalogue::Action, args: &Value) -> u32 {
    let player_once = args
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|a| a != "next" && a != "previous");
    if !action.repeatable || player_once {
        return 1;
    }
    // As f64 so a `3.0` from the model still counts; the cast saturates.
    args.get("times")
        .and_then(Value::as_f64)
        .map_or(1, |n| n.clamp(1.0, f64::from(MAX_TIMES)) as u32)
}

/// Runs `f` `times` times, at least once, stopping at the first error. The
/// error carries how many runs finished before it, so a burst that failed
/// partway can say how far it got.
fn repeat<T, E>(times: u32, mut f: impl FnMut() -> Result<T, E>) -> Result<T, (u32, E)> {
    let mut last = f().map_err(|e| (0, e))?;
    for done in 1..times {
        last = f().map_err(|e| (done, e))?;
    }
    Ok(last)
}

/// Sentinel `app` for tools backed by macOS itself, not a bundled app —
/// Spotlight search, output volume, revealing in Finder. These are always
/// present, so they are never gated. Using a real app name here would be wrong:
/// Finder lives in CoreServices and never appears in `installed_apps()`, so
/// gating on it silently dropped every Spotlight/volume/reveal tool.
pub(crate) const ALWAYS: &str = "macOS";

/// Whether a tool's backing app is available. The `ALWAYS` sentinel is
/// unconditionally true; otherwise a case-insensitive match on an installed
/// app's display name — the same names `frontmost_app()` reports, so "Music"
/// matches the app the user would name out loud.
#[cfg(target_os = "macos")]
fn app_installed(app: &str, installed: &[aloud_selection::InstalledApp]) -> bool {
    app == ALWAYS || installed.iter().any(|a| a.name.eq_ignore_ascii_case(app))
}

#[cfg(test)]
mod tests {

    /// "Press ⌘⇧5" should just happen; "quit Safari" by its keys or its menu
    /// must still ask.
    #[test]
    fn shortcuts_and_menu_commands_ask_only_when_they_destroy() {
        assert!(low_risk(
            "ALOUD_PRESS_KEYS",
            &json!({ "keys": "cmd+shift+5" })
        ));
        assert!(!low_risk("ALOUD_PRESS_KEYS", &json!({ "keys": "cmd+q" })));
        assert!(!low_risk(
            "ALOUD_PRESS_KEYS",
            &json!({ "keys": "nonsense+chord+x" })
        ));
        assert!(low_risk("ALOUD_MENU", &json!({ "item": "New Window" })));
        assert!(low_risk("ALOUD_MENU", &json!({ "list": true })));
        assert!(!low_risk("ALOUD_MENU", &json!({ "item": "Empty Trash…" })));
        assert!(!low_risk(
            "ALOUD_MENU",
            &json!({ "path": ["Safari", "Quit Safari"] })
        ));
    }

    #[test]
    fn writes_confirm_themselves_and_reads_leave_it_to_the_model() {
        assert_eq!(
            confirmation(
                "ALOUD_NOTES_CREATE",
                &json!({ "title": "Cake" }),
                &json!({ "created": true, "title": "Cake shopping list" })
            )
            .as_deref(),
            Some("Created the note “Cake shopping list”.")
        );
        assert_eq!(
            confirmation(
                "ALOUD_REMINDERS_ADD",
                &json!({ "text": "Call Ana", "due": "2026-10-03T17:00" }),
                &json!({ "added": true })
            )
            .as_deref(),
            Some("Added a reminder: Call Ana, due 2026-10-03 17:00.")
        );
        assert_eq!(
            confirmation("ALOUD_SPOTIFY", &json!({ "action": "pause" }), &json!({})).as_deref(),
            Some("Paused Spotify.")
        );
        assert_eq!(
            confirmation("ALOUD_FINDER_SEARCH", &json!({}), &json!({})),
            None
        );
        assert_eq!(
            confirmation(
                "ALOUD_OPEN_APP",
                &json!({ "names": ["Cursor"], "open": "components" }),
                &json!({ "opened": ["Cursor"], "not_installed": [], "target": "/x/components" })
            )
            .as_deref(),
            Some("Opened components in Cursor.")
        );
        // A bare open is often step one of a screen task; the run must go on.
        assert_eq!(
            confirmation(
                "ALOUD_OPEN_APP",
                &json!({ "names": ["Notes"] }),
                &json!({ "opened": ["Notes"], "not_installed": [] })
            ),
            None
        );
        // Candidates mean the model still has to ask.
        assert_eq!(
            confirmation(
                "ALOUD_OPEN_APP",
                &json!({ "names": ["Cursor"], "open": "components" }),
                &json!({ "opened": false, "candidates": ["/a/components", "/b/components"] })
            ),
            None
        );
        assert_eq!(
            confirmation(
                "ALOUD_SPOTIFY",
                &json!({ "action": "now_playing" }),
                &json!({})
            ),
            None
        );
        let repeated = |name, args| {
            let result = json!({ "done": 3, "times": 3, "last": {} });
            confirmation(name, &args, &result)
        };
        assert_eq!(
            repeated("ALOUD_PHOTO_BOOTH_TAKE_PHOTO", json!({ "times": 3 })).as_deref(),
            Some("Took 3 photos.")
        );
        assert_eq!(
            repeated("ALOUD_MUSIC", json!({ "action": "next", "times": 3 })).as_deref(),
            Some("Skipped 3 tracks.")
        );
        assert_eq!(
            repeated("ALOUD_PRESS_KEYS", json!({ "keys": "tab", "times": 3 })).as_deref(),
            Some("Pressed tab 3 times.")
        );
    }

    #[test]
    fn show_brings_back_the_note_the_file_and_what_was_opened() {
        let show = |name, args, result| show_target(name, &args, &result);
        let pair = |app: &str, target: &str| Some((app.to_string(), target.to_string()));
        assert_eq!(
            show(
                "ALOUD_NOTES_CREATE",
                json!({ "title": "Cake" }),
                json!({ "created": true, "id": "x-coredata://n/p1" })
            ),
            pair("Notes", "x-coredata://n/p1")
        );
        assert_eq!(
            show(
                "ALOUD_WRITE_FILE",
                json!({ "path": "Desktop/a.md" }),
                json!({ "wrote": "/Users/me/Desktop/a.md" })
            ),
            pair("Finder", "/Users/me/Desktop/a.md")
        );
        assert_eq!(
            show(
                "ALOUD_WRITE_FILE",
                json!({ "open_with": "cursor" }),
                json!({ "wrote": "/Users/me/a.md" })
            ),
            pair("Cursor", "/Users/me/a.md")
        );
        assert_eq!(
            show(
                "ALOUD_OPEN_APP",
                json!({ "names": ["Cursor"], "open": "components" }),
                json!({ "opened": ["Cursor"], "target": "/x/components" })
            ),
            pair("Cursor", "/x/components")
        );
        // A bare open, a question between candidates, a read: nothing to come back to.
        assert_eq!(
            show(
                "ALOUD_OPEN_APP",
                json!({ "names": ["Notes"] }),
                json!({ "opened": ["Notes"] })
            ),
            None
        );
        assert_eq!(
            show(
                "ALOUD_OPEN_APP",
                json!({}),
                json!({ "opened": false, "candidates": ["/a", "/b"] })
            ),
            None
        );
        assert_eq!(show("ALOUD_FINDER_SEARCH", json!({}), json!({})), None);
    }

    #[test]
    fn a_repeat_stops_at_the_first_failure_and_reports_progress() {
        let mut calls = 0;
        let outcome = repeat(5, || {
            calls += 1;
            if calls == 3 {
                Err("camera busy")
            } else {
                Ok(calls)
            }
        });
        assert_eq!(outcome, Err((2, "camera busy")));
        assert_eq!(calls, 3, "nothing may run after the failure");
        assert_eq!(repeat(3, || Ok::<_, ()>("shot")), Ok("shot"));
    }

    #[test]
    fn times_is_clamped_to_twenty() {
        let action = |name| catalogue::ACTIONS.iter().find(|a| a.name == name).unwrap();
        let photo = action("ALOUD_PHOTO_BOOTH_TAKE_PHOTO");
        assert_eq!(times(photo, &json!({ "times": 500 })), 20);
        assert_eq!(times(photo, &json!({ "times": 0 })), 1);
        assert_eq!(times(photo, &json!({ "times": -4 })), 1);
        assert_eq!(times(photo, &json!({ "times": 3.0 })), 3);
        assert_eq!(times(photo, &json!({})), 1);
        // A tool that is not repeatable, and a pause, run once whatever is asked.
        assert_eq!(times(action("ALOUD_OPEN_APP"), &json!({ "times": 3 })), 1);
        let music = action("ALOUD_MUSIC");
        assert_eq!(times(music, &json!({ "action": "pause", "times": 3 })), 1);
        assert_eq!(times(music, &json!({ "action": "next", "times": 3 })), 3);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn times_is_offered_only_on_repeatable_tools() {
        const REPEATABLE: &[&str] = &[
            "ALOUD_PHOTO_BOOTH_TAKE_PHOTO",
            "ALOUD_PRESS_KEYS",
            "ALOUD_MENU",
            "ALOUD_MUSIC",
            "ALOUD_SPOTIFY",
        ];
        let offered = tools(&[app("Music"), app("Spotify"), app("Photo Booth")]);
        for name in REPEATABLE {
            assert!(offered.iter().any(|t| t.name == *name), "{name} missing");
        }
        for tool in offered {
            let has_times = tool.parameters["properties"].get("times").is_some();
            assert_eq!(
                has_times,
                REPEATABLE.contains(&tool.name.as_str()),
                "{}",
                tool.name
            );
        }
    }

    #[test]
    fn sending_quitting_and_file_moves_still_ask_first() {
        for name in [
            "ALOUD_MESSAGES_SEND",
            "ALOUD_DISCORD_MESSAGE",
            "ALOUD_QUIT_APP",
            "ALOUD_COLLECT_FILES",
            "ALOUD_ORGANIZE_FOLDER",
        ] {
            assert!(
                !low_risk(name, &json!({})),
                "{name} must keep its confirmation"
            );
        }
        assert!(low_risk("ALOUD_OPEN_APP", &json!({})));
    }

    /// Creating a new file is undone by deleting it; overwriting one is not
    /// undone by anything, so that keeps its gate.
    #[test]
    fn only_a_brand_new_file_skips_the_write_gate() {
        let fresh = json!({ "path": "Desktop/aloud-test-never-exists-7f3a.md", "text": "x" });
        assert!(low_risk("ALOUD_WRITE_FILE", &fresh));
        let existing = json!({ "path": "Desktop", "text": "x" });
        assert!(!low_risk("ALOUD_WRITE_FILE", &existing));
        let appending = json!({ "path": "Desktop/aloud-test-never-exists-7f3a.md", "text": "x", "append": true });
        assert!(!low_risk("ALOUD_WRITE_FILE", &appending));
        let outside = json!({ "path": "/etc/aloud-test", "text": "x" });
        assert!(!low_risk("ALOUD_WRITE_FILE", &outside));
    }
    use super::*;
    use aloud_selection::InstalledApp;

    fn app(name: &str) -> InstalledApp {
        InstalledApp {
            name: name.to_string(),
            bundle_id: None,
            icon: None,
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn app_gated_tools_need_their_app() {
        // No Music installed → no Music tools.
        let names =
            |apps: &[InstalledApp]| tools(apps).into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert!(!names(&[]).iter().any(|n| n == "ALOUD_MUSIC"));
        // Music installed → its tools appear.
        assert!(names(&[app("Music")]).iter().any(|n| n == "ALOUD_MUSIC"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn system_wide_tools_are_always_offered() {
        // Spotlight/volume/reveal are built into macOS, so they must appear even
        // with zero apps installed. Regression: they were gated on "Finder",
        // which never appears in installed_apps(), so they silently vanished and
        // "find images" fell through to Composio.
        let names: Vec<_> = tools(&[]).into_iter().map(|t| t.name).collect();
        assert!(names.iter().any(|n| n == "ALOUD_FINDER_SEARCH"));
        assert!(names.iter().any(|n| n == "ALOUD_FINDER_REVEAL"));
        assert!(names.iter().any(|n| n == "ALOUD_SYSTEM_VOLUME"));
    }

    #[test]
    fn execute_returns_none_for_unknown_tools() {
        // The fall-through signal: a Composio slug is not ours.
        assert!(execute("GMAIL_SEND_EMAIL", &Value::Null).is_none());
    }
}
