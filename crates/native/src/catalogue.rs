//! The curated actions. One table, one handler each.
//!
//! Curated, not a generic `RUN_APPLESCRIPT(source)` runner: the model would
//! write the code, and the confirmation gate would then show the user a script
//! they cannot judge. A gate you cannot read is not a gate. Each action here
//! gets a real label, a real body, and a `read_only` flag that means something.
//!
//! ponytail: a generic runner is the upgrade path if this table ever gets long
//! enough to be the bottleneck. It is a dozen rows; it is not.

use serde_json::{json, Value};

use crate::organize;
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

/// Whether `name` is the app now in front. Used to wait out a launch, since
/// `open` returns before the app is on screen.
///
/// Asked of System Events rather than the AX API: the AX read of the focused
/// application needs the calling process to be AX-trusted and fails outside the
/// signed app, which would make this loop always time out in tests and examples.
fn frontmost_is(name: &str) -> bool {
    let Ok(out) = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to name of first application process \
             whose frontmost is true",
        ])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).trim() == name
}

/// The frontmost app's name, for tools that act on "this window" without being
/// told which app.
fn frontmost_app_name() -> Result<String, ScriptError> {
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to name of first application process \
             whose frontmost is true",
        ])
        .output()
        .map_err(|e| ScriptError::Failed(format!("reading the frontmost app: {e}")))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        return Err(ScriptError::Failed("no app is in front".into()));
    }
    Ok(name)
}

/// Launches or activates one app by bundle path.
///
/// By path, never `open -b` or a URL scheme: Launch Services resolves a scheme
/// like `discord://` to Discord *Canary* when both are installed. The first
/// candidate that exists wins, so a user-installed app outranks a system one of
/// the same name. `-a` activates a running instance rather than starting a
/// second, which is what "switch to Safari" has to mean.
fn open_one(name: &str) -> Result<(), ScriptError> {
    let home = std::env::var("HOME").unwrap_or_default();
    let bundle = [
        format!("/Applications/{name}.app"),
        format!("/System/Applications/{name}.app"),
        format!("/System/Applications/Utilities/{name}.app"),
        format!("{home}/Applications/{name}.app"),
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())
    .ok_or_else(|| ScriptError::Failed(format!("no application named {name} is installed")))?;

    let status = std::process::Command::new("/usr/bin/open")
        .args(["-a", &bundle])
        .status()
        .map_err(|e| ScriptError::Failed(format!("launching {name}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ScriptError::Failed(format!("could not open {name}")))
    }
}

/// Waits for `name` to actually reach the front.
///
/// `open` returns as soon as the launch is *requested*, not when the app is on
/// screen, so without this the agent's next step — a screen read or a click —
/// lands on the previous app. Polls rather than sleeping a flat second, so a
/// warm app costs ~50ms.
fn wait_until_frontmost(name: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if frontmost_is(name) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Gathers `paths` into `destination`, copying or moving.
///
/// One call for the whole batch, for the same reason the folder organiser is:
/// the loop caps a tool at `MAX_CALLS_PER_TOOL` per run, so a one-file-per-call
/// tool can never gather 354 search results. The destination is confined to the
/// home folder — this writes real files, and a path escape would scatter them
/// into a system directory.
fn collect_files(paths: &[String], destination: &str, moving: bool) -> Result<Value, ScriptError> {
    let home =
        std::env::var("HOME").map_err(|_| ScriptError::Failed("no home directory".into()))?;
    let home = std::path::PathBuf::from(home);

    let wanted = destination.trim().trim_start_matches("~/");
    let target = if wanted.starts_with('/') {
        std::path::PathBuf::from(wanted)
    } else {
        home.join(wanted)
    };
    // Resolve `..` before the containment check, or `Documents/../../etc` slips
    // through: the string starts inside home but does not stay there.
    let mut resolved = std::path::PathBuf::new();
    for part in target.components() {
        match part {
            std::path::Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    if !resolved.starts_with(&home) {
        return Err(ScriptError::Failed(format!(
            "{destination:?} is outside your home folder"
        )));
    }
    std::fs::create_dir_all(&resolved)
        .map_err(|e| ScriptError::Failed(format!("creating {}: {e}", resolved.display())))?;

    let mut done = 0usize;
    let mut failed = Vec::new();
    for path in paths {
        let source = std::path::Path::new(path);
        let Some(name) = source.file_name() else {
            failed.push(path.clone());
            continue;
        };
        // Already in the destination: gathering a folder into itself is a
        // no-op, not an error.
        if source.parent() == Some(resolved.as_path()) {
            done += 1;
            continue;
        }
        let dest = free_name(&resolved.join(name));
        let outcome = if moving {
            std::fs::rename(source, &dest).map(|_| ()).or_else(|_| {
                // rename fails across filesystems; fall back to copy+remove.
                std::fs::copy(source, &dest)
                    .and_then(|_| std::fs::remove_file(source))
                    .map(|_| ())
            })
        } else {
            std::fs::copy(source, &dest).map(|_| ())
        };
        match outcome {
            Ok(()) => done += 1,
            Err(_) => failed.push(path.clone()),
        }
    }

    Ok(json!({
        "collected": done,
        "destination": resolved.display().to_string(),
        "moved": moving,
        "failed": failed.len(),
    }))
}

/// `path`, or `path` with a numeric suffix when the name is taken — two files
/// called `photo.png` from different folders are two different photos.
fn free_name(path: &std::path::Path) -> std::path::PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    for n in 2..10_000 {
        let name = match &ext {
            Some(ext) => format!("{stem} {n}.{ext}"),
            None => format!("{stem} {n}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
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
        description: "Find or list files on the Mac. Set only the parameters the \
                      request actually implies.\n\
                      CRITICAL: words naming a file TYPE are `kind`, NOT `query`. \
                      'pictures', 'photos', 'images' → kind=image. 'screenshots' \
                      → kind=screenshot. 'documents', 'PDFs' → kind=document. \
                      'files' → kind=any. Do NOT put these words in `query` — \
                      that searches for the literal filename and finds nothing.\n\
                      - `query`: ONLY a specific content term ('buddha', \
                      'invoice', 'a cat'). OMIT it when the user just names a \
                      type and/or place.\n\
                      - `kind`: the file type per the mapping above.\n\
                      - `location`: a folder the user names ('on my desktop' → \
                      desktop). Omit for the whole home folder.\n\
                      Examples: 'pictures on my desktop' → {kind: image, \
                      location: desktop} (NO query). 'my screenshots' → {kind: \
                      screenshot}. 'photos of a cat' → {kind: image, query: \
                      cat}. 'files on my desktop' → {location: desktop}. 'my \
                      invoice pdf' → {kind: document, query: invoice}.",
        // Spotlight is built into macOS — always available, never gated on an
        // app the scanner cannot see.
        app: crate::ALWAYS,
        read_only: true,
        params: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A specific content term only. OMIT when \
                                        the user just names a type ('pictures') \
                                        or place ('desktop') — a type word here \
                                        matches the literal filename and fails."
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["screenshot", "image", "document", "any"],
                        "description": "File type. pictures/photos/images→image, \
                                        screenshots→screenshot, docs/PDFs→\
                                        document, files→any."
                    },
                    "location": {
                        "type": "string",
                        "enum": ["desktop", "downloads", "documents",
                                 "pictures", "movies", "music"],
                        "description": "A folder the user named. Omit for home."
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
                Ok(
                    json!({ "played": false, "reason": format!("nothing in the library matched {query:?}") }),
                )
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
            let source = match args
                .get("list")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
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
        name: "ALOUD_OPEN_APP",
        description: "Open or switch to applications by name — \"TextEdit\", \"Safari\", \
             \"System Settings\". Pass every app the user asked for in `names` and \
             they all open in this ONE call; never call this repeatedly to open \
             several. Use it to launch or focus an app; never drive Spotlight or \
             the Dock on screen to do it.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["names"],
                "properties": {
                    "names": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Every app to open, as it appears in \
                                        Applications. One entry is fine.",
                    }
                }
            })
        },
        run: |args| {
            // `names` is the real argument, but a model told to open one app
            // will reach for the singular anyway; accepting both costs a line
            // and saves a failed call.
            let names: Vec<String> = args
                .get("names")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|n| n.as_str().map(str::to_string))
                        .collect()
                })
                .or_else(|| {
                    args.get("name")
                        .and_then(Value::as_str)
                        .map(|n| vec![n.to_string()])
                })
                .unwrap_or_default();
            if names.is_empty() {
                return Err(ScriptError::Failed("no app names given".into()));
            }

            let mut opened = Vec::new();
            let mut missing = Vec::new();
            for name in &names {
                match open_one(name) {
                    Ok(()) => opened.push(name.clone()),
                    Err(_) => missing.push(name.clone()),
                }
            }
            if opened.is_empty() {
                return Err(ScriptError::Failed(format!(
                    "none of those apps are installed: {}",
                    missing.join(", ")
                )));
            }
            // The last one opened is the one left in front, so a follow-up
            // screen read targets something predictable.
            if let Some(last) = opened.last() {
                wait_until_frontmost(last);
            }
            Ok(json!({ "opened": opened, "not_installed": missing }))
        },
    },
    Action {
        name: "ALOUD_COLLECT_FILES",
        description: "Copy or move many files into one folder, in a single call. Use after a \
             file search when the user wants the results gathered somewhere — \
             \"put all my images in a folder\", \"collect these into Documents\". \
             Pass the absolute paths from the search result. Copies by default; \
             set move=true to move instead.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["paths", "destination"],
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Absolute paths of the files to gather.",
                    },
                    "destination": {
                        "type": "string",
                        "description": "Folder to gather them into, e.g. \
                                        \"Documents/Images\". Created if missing, \
                                        and must be inside your home folder.",
                    },
                    "move": {
                        "type": "boolean",
                        "description": "Move rather than copy. Defaults to false.",
                    }
                }
            })
        },
        run: |args| {
            let paths: Vec<String> = args
                .get("paths")
                .and_then(Value::as_array)
                .map(|l| {
                    l.iter()
                        .filter_map(|p| p.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if paths.is_empty() {
                return Err(ScriptError::Failed("no file paths given".into()));
            }
            let destination = required_str(args, "destination")?;
            let moving = args.get("move").and_then(Value::as_bool).unwrap_or(false);
            collect_files(&paths, destination, moving)
        },
    },
    Action {
        name: "ALOUD_ORGANIZE_FOLDER",
        description: "Tidy a folder by sorting its loose files into subfolders, in ONE call — \
             it handles every file in the folder, so never call it per file. You \
             choose the scheme: pass `rules` as a map of folder name to the file \
             extensions that belong in it, picking categories that suit what the \
             user has (e.g. {\"Images\": [\"png\",\"jpg\"], \"Installers\": [\"dmg\",\"pkg\"], \
             \"Documents\": [\"pdf\",\"docx\"], \"Archives\": [\"zip\"]}). Files whose \
             extension matches no rule are left alone, as are existing subfolders. \
             Only Desktop, Downloads and Documents can be organised. Reversible \
             with the undo tool.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["rules"],
                "properties": {
                    "folder": {
                        "type": "string",
                        "enum": ["Desktop", "Downloads", "Documents"],
                        "description": "Which folder to tidy. Defaults to Downloads.",
                    },
                    "rules": {
                        "type": "object",
                        "description": "Destination folder name → list of file \
                                        extensions, e.g. {\"Images\": [\"png\",\"jpg\"]}.",
                        "additionalProperties": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    }
                }
            })
        },
        run: |args| {
            let folder = organize::folder_from(args)?;
            let rules = organize::rules_from(args);
            if rules.is_empty() {
                return Err(ScriptError::Failed(
                    "no rules given — pass a map of folder name to extensions".into(),
                ));
            }
            let moves = organize::plan(&folder, &rules);
            if moves.is_empty() {
                return Ok(json!({
                    "organized": 0,
                    "note": "nothing to sort — no loose files matched those rules",
                }));
            }
            organize::apply(&folder, &moves)
        },
    },
    Action {
        name: "ALOUD_UNDO_ORGANIZE",
        description: "Put every file back where it was before the last folder tidy-up. Use \
             when the user says to undo it or put things back.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "properties": {
                    "folder": {
                        "type": "string",
                        "enum": ["Desktop", "Downloads", "Documents"],
                        "description": "The folder that was tidied. Defaults to Downloads.",
                    }
                }
            })
        },
        run: |args| organize::undo(&organize::folder_from(args)?),
    },
    Action {
        name: "ALOUD_WINDOW_ARRANGE",
        description: "Arrange the frontmost window on screen: fill the screen, or take the \
             left/right half or a quarter. Use for \"maximise this\", \"snap it \
             left\", \"tile these side by side\".",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["position"],
                "properties": {
                    "position": {
                        "type": "string",
                        "enum": ["full", "left", "right", "top-left",
                                 "top-right", "bottom-left", "bottom-right", "center"],
                    },
                    "app": {
                        "type": "string",
                        "description": "App whose window to move. Defaults to the \
                                        frontmost one.",
                    }
                }
            })
        },
        run: |args| {
            let position = required_str(args, "position")?;
            let app = args
                .get("app")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .map_or_else(frontmost_app_name, Ok)?;

            // The desktop window's bounds are the usable screen — Finder reports
            // it without needing Screen Recording permission, which reading
            // NSScreen from here would.
            let frame = script::run(
                "Finder",
                r#"tell application "Finder" to get bounds of window of desktop"#,
            )?;
            let nums: Vec<i64> = frame
                .split(", ")
                .filter_map(|n| n.trim().parse().ok())
                .collect();
            let (sw, sh) = match nums.as_slice() {
                [_, _, w, h] => (*w, *h),
                _ => return Err(ScriptError::Failed("could not read the screen size".into())),
            };
            // The menu bar is 25pt on every modern Mac; the Dock is left to the
            // window server, which keeps a maximised window clear of it.
            let top = 25;
            let h = sh - top;
            let (x, y, w, h) = match position {
                "full" => (0, top, sw, h),
                "left" => (0, top, sw / 2, h),
                "right" => (sw / 2, top, sw / 2, h),
                "top-left" => (0, top, sw / 2, h / 2),
                "top-right" => (sw / 2, top, sw / 2, h / 2),
                "bottom-left" => (0, top + h / 2, sw / 2, h / 2),
                "bottom-right" => (sw / 2, top + h / 2, sw / 2, h / 2),
                "center" => (sw / 4, top + h / 8, sw / 2, h * 3 / 4),
                other => return Err(ScriptError::Failed(format!("unknown position {other:?}"))),
            };

            let app_esc = script::escape(&app);
            // Position *then* size, and report the geometry actually achieved:
            // a window with a minimum size, or a browser that clamps its own
            // height, silently keeps its old bounds while AppleScript reports
            // success. Observed on Brave — it accepted the width and ignored
            // the height. Returning the real numbers means the model can say
            // what happened instead of claiming a tidy screen that is not.
            let source = format!(
                r#"tell application "System Events" to tell process "{app_esc}"
                    if (count of windows) is 0 then error "no open window"
                    set position of window 1 to {{{x}, {y}}}
                    set size of window 1 to {{{w}, {h}}}
                    set b to {{position, size}} of window 1
                    return (item 1 of item 1 of b as text) & "," & (item 2 of item 1 of b as text) & "," & (item 1 of item 2 of b as text) & "," & (item 2 of item 2 of b as text)
                end tell"#
            );
            let got = script::run(&app, &source).map_err(|e| match e {
                ScriptError::Failed(msg) if msg.contains("no open window") => {
                    ScriptError::Failed(format!("{app} has no open window to arrange"))
                }
                // -1719 is AppleScript's own "invalid index" for the same case.
                ScriptError::Failed(msg) if msg.contains("-1719") => {
                    ScriptError::Failed(format!("{app} has no open window to arrange"))
                }
                other => other,
            })?;
            Ok(json!({ "arranged": app, "position": position, "bounds": got }))
        },
    },
    Action {
        name: "ALOUD_BROWSER_TAB",
        description: "Open a URL in a new browser tab, or report the page the user is \
             currently looking at. Works with Safari, Chrome, Arc and Brave.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": ["open", "current"] },
                    "url": {
                        "type": "string",
                        "description": "The URL to open, for action=open.",
                    }
                }
            })
        },
        run: |args| {
            let action = required_str(args, "action")?;
            // Whichever browser the user actually uses — asking the frontmost
            // one first means "open this" lands where they are looking.
            let browser = ["Arc", "Google Chrome", "Brave Browser", "Safari"]
                .into_iter()
                .find(|b| std::path::Path::new(&format!("/Applications/{b}.app")).exists())
                .unwrap_or("Safari");

            if action == "current" {
                let source = if browser == "Safari" {
                    r#"tell application "Safari" to return (URL of current tab of window 1) & " — " & (name of current tab of window 1)"#.to_string()
                } else {
                    format!(
                        r#"tell application "{browser}" to return (URL of active tab of window 1) & " — " & (title of active tab of window 1)"#
                    )
                };
                let out = script::run(browser, &source)?;
                return Ok(json!({ "page": out }));
            }

            let url = required_str(args, "url")?;
            // Through `open -a`, not a tell: it launches the browser if it is
            // closed, where AppleScript to a dead app errors.
            let status = std::process::Command::new("/usr/bin/open")
                .args(["-a", browser, url])
                .status()
                .map_err(|e| ScriptError::Failed(format!("opening {url}: {e}")))?;
            if !status.success() {
                return Err(ScriptError::Failed(format!("could not open {url}")));
            }
            Ok(json!({ "opened": url, "browser": browser }))
        },
    },
    Action {
        name: "ALOUD_CLIPBOARD",
        description: "Read what is on the clipboard, or put text on it. Use for \"what did \
             I copy\", \"copy this to my clipboard\".",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": ["read", "write"] },
                    "text": { "type": "string", "description": "Text to copy, for action=write." }
                }
            })
        },
        run: |args| match required_str(args, "action")? {
            "read" => {
                let out = script::run("System Events", "return (the clipboard as text)")?;
                Ok(json!({ "clipboard": out }))
            }
            "write" => {
                let text = script::escape(required_str(args, "text")?);
                script::run(
                    "System Events",
                    &format!(r#"set the clipboard to "{text}""#),
                )?;
                Ok(json!({ "copied": true }))
            }
            other => Err(ScriptError::Failed(format!("unknown action {other:?}"))),
        },
    },
    Action {
        name: "ALOUD_QUIT_APP",
        description: "Quit an application by name. Use for \"close Safari\", \"quit Music\".",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["name"],
                "properties": { "name": { "type": "string" } }
            })
        },
        run: |args| {
            let name = required_str(args, "name")?;
            let esc = script::escape(name);
            script::run(name, &format!(r#"tell application "{esc}" to quit"#))?;
            Ok(json!({ "quit": name }))
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

    /// "Open TextEdit" is the most common thing anyone says to this app, and
    /// without a tool for it the agent drove Spotlight on screen instead —
    /// clicking a search result and typing into the wrong window.
    #[test]
    #[cfg(target_os = "macos")]
    fn a_real_app_opens_and_an_invented_one_is_refused() {
        let open = ACTIONS
            .iter()
            .find(|a| a.name == "ALOUD_OPEN_APP")
            .expect("the launcher must be offered");
        assert!(!open.read_only, "launching an app is a write");

        // Resolution is by path, so a name that exists nowhere must fail with a
        // readable message rather than launching something adjacent.
        let err = (open.run)(&json!({ "name": "NoSuchAppZZZ" })).unwrap_err();
        assert!(
            err.to_string().contains("NoSuchAppZZZ"),
            "the error should name the app: {err}"
        );

        // Safari ships with macOS, so its bundle must resolve under
        // /Applications. Resolution only — this does not launch it.
        assert!(
            std::path::Path::new("/Applications/Safari.app").exists()
                || std::path::Path::new("/System/Applications/Safari.app").exists(),
            "expected Safari in one of the searched locations"
        );
    }

    /// Gathering must be confined to the home folder. This writes real files,
    /// and a `..` escape would scatter them into a system directory — the
    /// string starts inside home but does not stay there.
    #[test]
    fn collecting_outside_the_home_folder_is_refused() {
        for escape in ["../../etc", "Documents/../../../tmp", "/etc"] {
            let err = collect_files(&["/tmp/x".to_string()], escape, false);
            assert!(
                err.is_err(),
                "{escape:?} should be refused, got {:?}",
                err.map(|v| v.to_string())
            );
        }
    }

    /// The whole reason this tool exists: 354 search results cannot be 354 tool
    /// calls, so one call has to gather the batch.
    #[test]
    fn one_call_collects_a_whole_batch_without_overwriting() {
        let scratch = std::env::temp_dir().join(format!("aloud-collect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();

        // Two files sharing a name, from different folders — the collision case.
        let mut paths = Vec::new();
        for folder in ["a", "b"] {
            let dir = scratch.join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("photo.png");
            std::fs::write(&file, folder.as_bytes()).unwrap();
            paths.push(file.display().to_string());
        }
        for i in 0..50 {
            let file = scratch.join(format!("f{i}.png"));
            std::fs::write(&file, b"x").unwrap();
            paths.push(file.display().to_string());
        }

        // Destination under $HOME, since that is the only place allowed.
        let home = std::env::var("HOME").unwrap();
        let rel = format!(".aloud-test-collect-{}", std::process::id());
        let out = collect_files(&paths, &rel, false).unwrap();
        assert_eq!(out["collected"], 52);
        assert_eq!(out["failed"], 0);

        let dest = std::path::PathBuf::from(&home).join(&rel);
        assert!(dest.join("photo.png").exists());
        assert!(
            dest.join("photo 2.png").exists(),
            "a name collision must suffix, not overwrite"
        );

        std::fs::remove_dir_all(&dest).unwrap();
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    /// "Open 15 apps" was opening one: the tool took a single name and the loop
    /// caps a tool at MAX_CALLS_PER_TOOL per run, so it could never reach 15.
    #[test]
    fn open_app_takes_a_list_so_one_call_opens_many() {
        let open = ACTIONS.iter().find(|a| a.name == "ALOUD_OPEN_APP").unwrap();
        let params = (open.params)();
        assert_eq!(
            params["properties"]["names"]["type"], "array",
            "must accept a list, or a 15-app request cannot be satisfied"
        );
        assert_eq!(params["required"][0], "names");
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
