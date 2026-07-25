//! The curated actions. One table, one handler each.
//!
//! Curated, not a generic `RUN_APPLESCRIPT(source)` runner: the model would
//! write the code, and the confirmation gate would then show the user a script
//! they cannot judge. A gate you cannot read is not a gate. Each action here
//! gets a real label, a real body, and a `read_only` flag that means something.
//!
//! ponytail: a generic runner is the upgrade path if this table ever gets long
//! enough to be the bottleneck. It is eight rows; it is not.

use serde_json::{json, Value};

use crate::script::{self, ScriptError};
use crate::search;

/// Pulls a required string argument, or a readable error the model can recover
/// from — a missing field is the model's mistake, not a permission problem.
fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ScriptError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ScriptError::Failed(format!("missing required argument {key:?}")))
}

/// One native action. `params` is the JSON-schema object handed to the model;
/// `run` is called with the model's (already gate-approved, for writes)
/// arguments.
pub struct Action {
    pub name: &'static str,
    pub description: &'static str,
    /// The macOS app this controls, and the app whose `installed` presence
    /// gates the tool. Also what a -1743 denial names.
    pub app: &'static str,
    pub read_only: bool,
    pub params: fn() -> Value,
    pub run: fn(&Value) -> Result<Value, ScriptError>,
}

/// Every native action. Filtered by installed app in `lib::tools`. `app`s that
/// ship with macOS (Music, Reminders, Notes, Messages, Finder) are effectively
/// always installed; the filter still runs so a machine that removed one does
/// not offer its tools.
pub const ACTIONS: &[Action] = &[
    Action {
        name: "ALOUD_FINDER_SEARCH",
        description: "Search the Mac for files by name, by text seen INSIDE \
                      images (OCR), by objects/scenes in photos, and by kind. \
                      Use for 'find my …' requests. `kind` may be 'image', \
                      'document', or 'any' (default).",
        // Spotlight is built into macOS — always available, never gated on an
        // app the scanner cannot see.
        app: crate::ALWAYS,
        read_only: true,
        params: || {
            json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for, e.g. 'buddha', 'invoice', \
                                        'tax 2025'."
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["image", "document", "any"],
                        "description": "Restrict to a file kind; defaults to any."
                    }
                }
            })
        },
        run: search::run,
    },
    Action {
        name: "ALOUD_MUSIC_NOW_PLAYING",
        description: "Get the name and artist of the track Music is currently \
                      playing, or report that nothing is playing.",
        app: "Music",
        read_only: true,
        params: || json!({ "type": "object", "properties": {} }),
        run: |_args| {
            // `player state` avoids raising an error when nothing is loaded.
            let out = script::run(
                "Music",
                r#"tell application "Music"
                    if player state is stopped then return "stopped"
                    return (get name of current track) & " — " & (get artist of current track)
                end tell"#,
            )?;
            if out == "stopped" {
                Ok(json!({ "playing": false }))
            } else {
                Ok(json!({ "playing": true, "track": out }))
            }
        },
    },
    Action {
        name: "ALOUD_MUSIC_PLAY",
        description: "Start or resume playback in Music. If `query` is given, \
                      search the library and play the best match; otherwise \
                      resume whatever is loaded.",
        app: "Music",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional song, album, or artist to search \
                                        the library for and play."
                    }
                }
            })
        },
        run: |args| {
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            if query.is_empty() {
                script::run("Music", r#"tell application "Music" to play"#)?;
                return Ok(json!({ "played": "resumed" }));
            }
            // Play the first library track whose name or artist matches. `whose`
            // filters in-app, so no track list crosses the AppleScript bridge.
            let q = script::escape(query);
            let source = format!(
                r#"tell application "Music"
                    set hits to (every track of library playlist 1 whose name contains "{q}" or artist contains "{q}")
                    if hits is {{}} then return "none"
                    play (item 1 of hits)
                    return (get name of current track) & " — " & (get artist of current track)
                end tell"#
            );
            let out = script::run("Music", &source)?;
            if out == "none" {
                Ok(json!({ "played": false, "reason": format!("nothing in the library matched {query:?}") }))
            } else {
                Ok(json!({ "played": true, "track": out }))
            }
        },
    },
    Action {
        name: "ALOUD_MUSIC_PAUSE",
        description: "Pause playback in Music.",
        app: "Music",
        read_only: false,
        params: || json!({ "type": "object", "properties": {} }),
        run: |_args| {
            script::run("Music", r#"tell application "Music" to pause"#)?;
            Ok(json!({ "paused": true }))
        },
    },
    Action {
        name: "ALOUD_MUSIC_NEXT",
        description: "Skip to the next track in Music.",
        app: "Music",
        read_only: false,
        params: || json!({ "type": "object", "properties": {} }),
        run: |_args| {
            let out = script::run(
                "Music",
                r#"tell application "Music"
                    next track
                    return (get name of current track) & " — " & (get artist of current track)
                end tell"#,
            )?;
            Ok(json!({ "skipped": true, "track": out }))
        },
    },
    Action {
        name: "ALOUD_REMINDERS_ADD",
        description: "Add a reminder to the Reminders app. `list` is optional; \
                      omit it to use the default list.",
        app: "Reminders",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string", "description": "The reminder text." },
                    "list": {
                        "type": "string",
                        "description": "Optional named list; defaults to the default list."
                    }
                }
            })
        },
        run: |args| {
            let text = required_str(args, "text")?;
            let text = script::escape(text);
            let source = match args.get("list").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                Some(list) => {
                    let list = script::escape(list);
                    format!(
                        r#"tell application "Reminders"
                            make new reminder at end of list "{list}" with properties {{name:"{text}"}}
                        end tell"#
                    )
                }
                None => format!(
                    r#"tell application "Reminders"
                        make new reminder with properties {{name:"{text}"}}
                    end tell"#
                ),
            };
            script::run("Reminders", &source)?;
            Ok(json!({ "added": true }))
        },
    },
    Action {
        name: "ALOUD_NOTES_APPEND",
        description: "Append a line to a note by title in the Notes app, \
                      creating the note if it does not exist.",
        app: "Notes",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["title", "text"],
                "properties": {
                    "title": { "type": "string", "description": "The note's title." },
                    "text": { "type": "string", "description": "The line to append." }
                }
            })
        },
        run: |args| {
            let title = script::escape(required_str(args, "title")?);
            let text = script::escape(required_str(args, "text")?);
            // Notes bodies are HTML; a <br> keeps the appended line on its own
            // line. Create-if-missing so "add to my Groceries note" works even
            // the first time.
            let source = format!(
                r#"tell application "Notes"
                    if not (exists note "{title}") then
                        make new note with properties {{name:"{title}", body:"{text}"}}
                    else
                        set n to note "{title}"
                        set body of n to (body of n) & "<br>{text}"
                    end if
                end tell"#
            );
            script::run("Notes", &source)?;
            Ok(json!({ "appended": true }))
        },
    },
    Action {
        name: "ALOUD_MESSAGES_SEND",
        description: "Send an iMessage or SMS to a phone number or email handle. \
                      The `to` field must be a full handle, never a name — resolve \
                      names to a handle first, or ask.",
        app: "Messages",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["to", "text"],
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "The FULL recipient handle: a phone number \
                                        (+15551234567) or email. Not a display name."
                    },
                    "text": { "type": "string", "description": "The message body." }
                }
            })
        },
        run: |args| {
            let to = required_str(args, "to")?;
            let text = required_str(args, "text")?;
            let to_esc = script::escape(to);
            let text_esc = script::escape(text);
            // `buddy` on the iMessage service. If the handle is an SMS-only
            // number, Messages routes it — no need to pick the service here.
            let source = format!(
                r#"tell application "Messages"
                    set svc to 1st service whose service type = iMessage
                    send "{text_esc}" to buddy "{to_esc}" of svc
                end tell"#
            );
            script::run("Messages", &source)?;
            // Echo the handle back so a mis-send is visible in the transcript.
            Ok(json!({ "sent": true, "to": to }))
        },
    },
    Action {
        name: "ALOUD_FINDER_REVEAL",
        description: "Reveal a file or folder in Finder by its absolute path.",
        // Finder is always running on macOS but never appears in installed_apps.
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string", "description": "Absolute POSIX path." }
                }
            })
        },
        run: |args| {
            let path = script::escape(required_str(args, "path")?);
            let source = format!(
                r#"tell application "Finder"
                    reveal (POSIX file "{path}" as alias)
                    activate
                end tell"#
            );
            script::run("Finder", &source)?;
            Ok(json!({ "revealed": true }))
        },
    },
    Action {
        name: "ALOUD_SYSTEM_VOLUME",
        description: "Set the system output volume to a level from 0 to 100.",
        // A system-wide action with no app of its own.
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["level"],
                "properties": {
                    "level": {
                        "type": "integer", "minimum": 0, "maximum": 100,
                        "description": "Output volume, 0 (mute) to 100 (max)."
                    }
                }
            })
        },
        run: |args| {
            let level = args
                .get("level")
                .and_then(Value::as_i64)
                .ok_or_else(|| ScriptError::Failed("level must be a number 0–100".into()))?
                .clamp(0, 100);
            script::run(
                "System Events",
                &format!("set volume output volume {level}"),
            )?;
            Ok(json!({ "volume": level }))
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_flags_are_honest() {
        // A write mislabelled read-only silently skips the confirmation gate —
        // the one non-negotiable property in this codebase. NOW_PLAYING reads;
        // PLAY writes.
        let now = ACTIONS
            .iter()
            .find(|a| a.name == "ALOUD_MUSIC_NOW_PLAYING")
            .unwrap();
        assert!(now.read_only, "NOW_PLAYING must be read-only");

        let play = ACTIONS
            .iter()
            .find(|a| a.name == "ALOUD_MUSIC_PLAY")
            .unwrap();
        assert!(!play.read_only, "PLAY changes playback — must be a write");
    }

    #[test]
    fn params_are_valid_schema_objects() {
        for action in ACTIONS {
            let params = (action.params)();
            assert_eq!(
                params["type"], "object",
                "{}: params must be a JSON-schema object",
                action.name
            );
            assert!(
                params.get("properties").is_some(),
                "{}: params must declare properties",
                action.name
            );
        }
    }

    #[test]
    fn names_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for action in ACTIONS {
            assert!(
                action.name.starts_with("ALOUD_"),
                "{} must carry the ALOUD_ prefix so dispatch can find it",
                action.name
            );
            assert!(seen.insert(action.name), "duplicate action {}", action.name);
        }
    }
}
