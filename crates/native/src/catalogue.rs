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
use crate::{media, notes, reminders};

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
    let bundle = bundle_path(name)?;
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

/// Where an installed app's bundle is, user-installed first.
fn bundle_path(name: &str) -> Result<String, ScriptError> {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        format!("/Applications/{name}.app"),
        format!("/System/Applications/{name}.app"),
        format!("/System/Applications/Utilities/{name}.app"),
        format!("{home}/Applications/{name}.app"),
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())
    .ok_or_else(|| ScriptError::Failed(format!("no application named {name} is installed")))
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
    let resolved = under_home(destination)?;
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

fn home_dir() -> Result<std::path::PathBuf, ScriptError> {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .map_err(|_| ScriptError::Failed("no home directory".into()))
}

/// Resolves a user-supplied path against the home folder and refuses anything
/// that escapes it.
///
/// `..` is resolved *before* the containment check: `Documents/../../etc` starts
/// with the home prefix as a string but does not stay inside it, so a naive
/// `starts_with` on the raw path would pass it. These tools write and move real
/// files, so this is the boundary that matters.
fn under_home(path: &str) -> Result<std::path::PathBuf, ScriptError> {
    let home = home_dir()?;
    let wanted = path.trim().trim_start_matches("~/");
    let target = if wanted.starts_with('/') {
        std::path::PathBuf::from(wanted)
    } else {
        home.join(wanted)
    };

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
            "{path:?} is outside your home folder"
        )));
    }
    Ok(resolved)
}

/// Writes `text` to a path under the home folder, creating parent folders.
fn write_home_file(path: &str, text: &str, append: bool) -> Result<Value, ScriptError> {
    let resolved = under_home(path)?;
    // A path the user names as a folder would otherwise fail cryptically inside
    // the write; saying so is more useful than an OS error.
    if resolved.is_dir() {
        return Err(ScriptError::Failed(format!(
            "{path:?} is a folder, not a file — give it a filename"
        )));
    }
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ScriptError::Failed(format!("creating {}: {e}", parent.display())))?;
    }

    let existed = resolved.exists();
    if append && existed {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&resolved)
            .map_err(|e| ScriptError::Failed(format!("opening {}: {e}", resolved.display())))?;
        file.write_all(text.as_bytes())
            .map_err(|e| ScriptError::Failed(format!("writing {}: {e}", resolved.display())))?;
    } else {
        std::fs::write(&resolved, text)
            .map_err(|e| ScriptError::Failed(format!("writing {}: {e}", resolved.display())))?;
    }

    Ok(json!({
        "wrote": resolved.display().to_string(),
        "bytes": text.len(),
        "appended": append && existed,
    }))
}

/// Whether a write would only create a file that does not exist yet — the one
/// file write too small to confirm, since deleting it undoes it. Overwriting or
/// appending to an existing file still asks.
pub(crate) fn creates_new_file(args: &Value) -> bool {
    let append = args.get("append").and_then(Value::as_bool).unwrap_or(false);
    let path = args.get("path").and_then(Value::as_str).unwrap_or_default();
    !append && !path.is_empty() && under_home(path).is_ok_and(|p| !p.exists())
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
        name: "ALOUD_MUSIC",
        description: "Control the Music app: play (with `query`, play the best match from the \
                      library; without, resume), pause, next, previous, or now_playing.",
        app: "Music",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": ["play", "pause", "next", "previous", "now_playing"] },
                    "query": { "type": "string", "description": "For play: a song, album or artist in the library." }
                }
            })
        },
        run: |args| {
            let action = required_str(args, "action")?;
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // `whose` filters in-app, so no track list crosses the bridge; `player state` avoids an
            // error when nothing is loaded.
            let out = script::run_jxa(
                "Music",
                r#"const M = Application("Music");
                const now = () => M.playerState() === "stopped" ? null : M.currentTrack.name() + " — " + M.currentTrack.artist();
                if (ARG.action === "play" && ARG.query) {
                    const hits = M.libraryPlaylists[0].tracks.whose({ _or: [{ name: { _contains: ARG.query } }, { artist: { _contains: ARG.query } }] })();
                    if (!hits.length) return JSON.stringify({ played: false, reason: "nothing in the library matched " + ARG.query });
                    hits[0].play();
                } else if (ARG.action === "play") M.play();
                else if (ARG.action === "pause") M.pause();
                else if (ARG.action === "next") M.nextTrack();
                else if (ARG.action === "previous") M.previousTrack();
                // The player reports its old state for a moment after a command; reading it at
                // once had a pause answer "playing".
                if (ARG.action === "pause") return JSON.stringify({ paused: true });
                if (ARG.action !== "now_playing") delay(0.6);
                return JSON.stringify({ state: M.playerState(), track: now() });"#,
                &json!({ "action": action, "query": query }),
            )?;
            Ok(serde_json::from_str(&out).unwrap_or_else(|_| json!({ "done": action })))
        },
    },
    Action {
        name: "ALOUD_SPOTIFY",
        description: "Control the Spotify app: play, pause, next, previous, or now_playing. To \
                      play something specific give `uri` (a spotify: URI or an open.spotify.com \
                      link). With only `query`, Spotify opens its search for it, since the app \
                      cannot search on its own — say so in your answer.",
        app: "Spotify",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": ["play", "pause", "next", "previous", "now_playing"] },
                    "uri": { "type": "string", "description": "spotify:… URI or open.spotify.com link to play." },
                    "query": { "type": "string", "description": "What to search for when there is no URI." }
                }
            })
        },
        run: |args| {
            let action = required_str(args, "action")?;
            let uri = args
                .get("uri")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|link| {
                    media::spotify_uri(link).ok_or_else(|| {
                        ScriptError::Failed(format!("{link:?} is not a Spotify link or URI"))
                    })
                })
                .transpose()?;
            // ponytail: Spotify's scripting has no search, so a bare query opens the search page and
            // stops there. A connected Spotify search (Web API via Composio) that returns a URI is
            // the upgrade; one-off users of dev-mode apps are capped at 5 since Feb 2026.
            let out = script::run_jxa(
                "Spotify",
                r#"const S = Application("Spotify");
                const now = () => { try { return S.currentTrack.name() + " — " + S.currentTrack.artist(); } catch (e) { return null; } };
                if (ARG.action === "play" && ARG.uri) S.playTrack(ARG.uri);
                else if (ARG.action === "play" && ARG.query) {
                    const app = Application.currentApplication(); app.includeStandardAdditions = true;
                    app.openLocation("spotify:search:" + encodeURIComponent(ARG.query));
                    return JSON.stringify({ searched: ARG.query, note: "opened Spotify's search; pick a result to play" });
                }
                else if (ARG.action === "play") S.play();
                else if (ARG.action === "pause") S.pause();
                else if (ARG.action === "next") S.nextTrack();
                else if (ARG.action === "previous") S.previousTrack();
                // The player reports its old state for a moment after a command; reading it at
                // once had a pause answer "playing".
                if (ARG.action === "pause") return JSON.stringify({ paused: true });
                if (ARG.action !== "now_playing") delay(0.6);
                return JSON.stringify({ state: S.playerState(), track: now() });"#,
                &json!({ "action": action, "uri": uri, "query": args.get("query").and_then(Value::as_str) }),
            )?;
            Ok(serde_json::from_str(&out).unwrap_or_else(|_| json!({ "done": action })))
        },
    },
    Action {
        name: "ALOUD_REMINDERS_ADD",
        description: "Add a reminder to the Reminders app. Give `due` whenever the user says \
                      when (\"at 5\", \"tomorrow morning\", \"on Friday\"), worked out from \
                      today's date: YYYY-MM-DDTHH:MM in local time, or YYYY-MM-DD for a whole \
                      day. `list` is optional; omit it for the default list.",
        app: "Reminders",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": { "type": "string", "description": "What to be reminded of." },
                    "due": {
                        "type": "string",
                        "description": "When, local time: 2026-10-03T17:00, or 2026-10-03 for all day."
                    },
                    "list": {
                        "type": "string",
                        "description": "Optional named list; defaults to the default list."
                    }
                }
            })
        },
        run: |args| {
            let text = required_str(args, "text")?;
            let due = match args
                .get("due")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(due) => {
                    let due = reminders::due_parts(due).map_err(ScriptError::Failed)?;
                    let (hour, minute) = due.time.unzip();
                    json!([due.year, due.month, due.day, hour, minute])
                }
                None => Value::Null,
            };
            // `new Date(...)` sets every field at once, so a validated date cannot roll over the way
            // AppleScript's field-by-field dates do.
            let out = script::run_jxa(
                "Reminders",
                r#"const R = Application("Reminders");
                const list = ARG.list ? R.lists.byName(ARG.list) : R.defaultList();
                const props = { name: ARG.text };
                if (ARG.due) {
                    const [y, m, d, h, mi] = ARG.due;
                    if (h === null) props.alldayDueDate = new Date(y, m - 1, d);
                    else props.dueDate = props.remindMeDate = new Date(y, m - 1, d, h, mi);
                }
                list.reminders.push(R.Reminder(props));
                return list.name();"#,
                &json!({ "text": text, "due": due, "list": args.get("list").and_then(Value::as_str).filter(|s| !s.is_empty()) }),
            )?;
            Ok(json!({ "added": true, "list": out }))
        },
    },
    Action {
        name: "ALOUD_NOTES_CREATE",
        description: "Create a new note in the Notes app and open it. Write the content yourself \
                      into `body` as markdown (lines, `- ` bullets, `1.` steps, `## ` headings): \
                      for \"make a shopping list for a cake and put it in a note\", body is the \
                      actual list. Plain text, no emoji. Omit `body` for an empty note.",
        app: "Notes",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["title"],
                "properties": {
                    "title": { "type": "string", "description": "The note's title." },
                    "body": { "type": "string", "description": "The note's content, as markdown." }
                }
            })
        },
        run: |args| {
            let title = required_str(args, "title")?;
            let body = args.get("body").and_then(Value::as_str).unwrap_or_default();
            let name = script::run_jxa(
                "Notes",
                r#"const Notes = Application("Notes");
                const note = Notes.make({ new: "note", withProperties: { body: ARG.body } });
                Notes.activate();
                Notes.show(note);
                return note.name();"#,
                &json!({ "body": notes::note_body(title, body) }),
            )?;
            Ok(json!({ "created": true, "title": name }))
        },
    },
    Action {
        name: "ALOUD_NOTES_APPEND",
        description:
            "Add to an existing note in the Notes app — \"add eggs to my shopping note\", \
                      \"add this to my note\". `title` is any part of the note's name; omit it to \
                      add to the note open in Notes (else the most recent). A note that doesn't \
                      exist yet is created. Write the content yourself into `body` as markdown.",
        app: "Notes",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["body"],
                "properties": {
                    "title": { "type": "string", "description": "Part of the note's name." },
                    "body": { "type": "string", "description": "What to add, as markdown." }
                }
            })
        },
        run: |args| {
            let body = required_str(args, "body")?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            // Appending rewrites the whole body, and attachments are not in the HTML a script
            // reads, so a note with any is refused rather than silently stripped of them.
            let out = script::run_jxa(
                "Notes",
                r#"const Notes = Application("Notes");
                let note = null;
                if (ARG.title) {
                    const found = Notes.notes.whose({ name: { _contains: ARG.title } })();
                    note = found.length ? found[0] : null;
                } else {
                    try { const picked = Notes.selection(); if (picked.length) note = picked[0]; } catch (e) {}
                    if (!note && Notes.notes.length) note = Notes.notes[0];
                }
                if (!note) {
                    note = Notes.make({ new: "note", withProperties: { body: ARG.fresh } });
                    Notes.activate(); Notes.show(note);
                    return JSON.stringify({ created: true, title: note.name() });
                }
                if (note.attachments.length > 0) {
                    throw new Error("“" + note.name() + "” has images or files in it, which rewriting it would delete; add this to a different note.");
                }
                // Notes joins a list to the one right before it: more bullets under a shopping list
                // is right, bullets becoming steps 3 and 4 of a numbered list is not.
                const old = note.body().trimEnd();
                const clash = (old.endsWith("</ol>") && ARG.html.startsWith("<ul>"))
                    || (old.endsWith("</ul>") && ARG.html.startsWith("<ol>"));
                note.body = old + (clash ? "<div><br></div>" : "") + ARG.html;
                Notes.activate(); Notes.show(note);
                return JSON.stringify({ appended: true, title: note.name() });"#,
                &json!({
                    "title": title,
                    "html": notes::to_html(body),
                    "fresh": notes::note_body(title.unwrap_or("Note"), body),
                }),
            )?;
            Ok(serde_json::from_str(&out).unwrap_or_else(|_| json!({ "appended": true })))
        },
    },
    Action {
        name: "ALOUD_PHOTO_BOOTH_TAKE_PHOTO",
        description: "Take a photo with Photo Booth: opens it and presses Take Photo (a short \
                      countdown, then the shutter).",
        app: "Photo Booth",
        read_only: false,
        params: || json!({ "type": "object", "properties": {} }),
        run: |_args| {
            open_one("Photo Booth")?;
            wait_until_frontmost("Photo Booth");
            // The camera takes a moment to come up; a press before it does is ignored.
            std::thread::sleep(std::time::Duration::from_millis(1500));
            // Photo Booth has no scripting dictionary, so its own control is pressed through
            // System Events: the menu item first, the window button if the menu lacks it.
            // The menu walk is macos-automator-mcp's jxa_ui_menu_actions fallback (MIT).
            let how = script::run_jxa(
                "System Events",
                r#"const proc = Application("System Events").processes.byName("Photo Booth");
                for (const bar of proc.menuBars[0].menuBarItems()) {
                    const hits = bar.menus[0].menuItems.whose({ name: { _beginsWith: "Take" } })();
                    if (hits.length && hits[0].enabled()) { hits[0].click(); return "menu " + hits[0].name(); }
                }
                const buttons = proc.windows[0].entireContents().filter((e) => {
                    try { return e.role() === "AXButton" && /take/i.test(e.description() + e.name()); } catch (x) { return false; }
                });
                if (buttons.length) { buttons[0].click(); return "button"; }
                throw new Error("Photo Booth showed no Take Photo control");"#,
                &json!({}),
            )?;
            Ok(json!({ "took_photo": true, "via": how }))
        },
    },
    Action {
        name: "ALOUD_DISCORD_MESSAGE",
        description: "Send a Discord message to a person or channel by name: opens Discord's \
                      quick switcher, goes to `to`, and sends `text`. Write the message yourself.",
        app: "Discord",
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["to", "text"],
                "properties": {
                    "to": { "type": "string", "description": "The person's or channel's name as it appears in Discord." },
                    "text": { "type": "string", "description": "The message to send." }
                }
            })
        },
        run: |args| {
            let to = required_str(args, "to")?;
            let text = required_str(args, "text")?;
            open_one("Discord")?;
            wait_until_frontmost("Discord");
            // Discord has no scripting dictionary; its ⌘K switcher is the reliable way in. The
            // window title is checked before typing: a fuzzy switcher match on the wrong person
            // would otherwise get the message. Newlines would press Return mid-message.
            // ponytail: keystrokes can mistype non-ASCII; pasting through aloud_paste is the upgrade.
            let out = script::run_jxa(
                "System Events",
                r#"const se = Application("System Events");
                const proc = se.processes.byName("Discord");
                se.keystroke("k", { using: "command down" }); delay(0.5);
                se.keystroke(ARG.to); delay(0.8);
                se.keyCode(36); delay(1.0);
                const title = proc.windows[0].name();
                if (!title.toLowerCase().includes(ARG.to.toLowerCase())) {
                    se.keyCode(53);
                    throw new Error("Discord opened “" + title + "”, not " + ARG.to + "; nothing was sent.");
                }
                se.keystroke(ARG.text.replace(/\s*\n\s*/g, " "));
                if (!ARG.dry_run) se.keyCode(36);
                return title;"#,
                &json!({ "to": to, "text": text, "dry_run": args.get("dry_run").and_then(Value::as_bool).unwrap_or(false) }),
            )?;
            Ok(
                json!({ "sent": !args.get("dry_run").and_then(Value::as_bool).unwrap_or(false), "to": out }),
            )
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
        name: "ALOUD_WRITE_FILE",
        description: "Write text to a file, creating it if it does not exist. Use when the \
             user asks to save, write or note something down as a file — a summary, \
             a list, notes, code. Give the whole text in one call; it replaces the file's \
             contents unless append is set. To make a file in an editor (\"open Cursor and \
             create todo.md\"), set open_with. When no folder is named, write to Desktop/ \
             and say so.",
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "required": ["path", "text"],
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Where to write, relative to the home folder \
                                        — \"Desktop/summary.md\", \"Documents/notes.txt\". \
                                        Must be inside the home folder.",
                    },
                    "text": { "type": "string", "description": "The full contents to write." },
                    "append": {
                        "type": "boolean",
                        "description": "Add to the end instead of replacing. Defaults to false.",
                    },
                    "open_with": {
                        "type": "string",
                        "enum": ["cursor", "vscode", "default"],
                        "description": "Open the file afterwards in this editor, or the default app.",
                    }
                }
            })
        },
        run: |args| {
            let path = required_str(args, "path")?;
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            let append = args.get("append").and_then(Value::as_bool).unwrap_or(false);
            let mut wrote = write_home_file(path, text, append)?;
            if let Some(with) = args.get("open_with").and_then(Value::as_str) {
                let file = wrote["wrote"].as_str().unwrap_or_default().to_string();
                // By bundle path, not the `cursor` CLI: a Finder-launched app has a PATH without it.
                let mut open = std::process::Command::new("/usr/bin/open");
                match with {
                    "cursor" => open.args(["-a", &bundle_path("Cursor")?]),
                    "vscode" => open.args(["-a", &bundle_path("Visual Studio Code")?]),
                    _ => &mut open,
                };
                let opened = open.arg(&file).status().is_ok_and(|s| s.success());
                wrote["opened"] = json!(opened);
            }
            Ok(wrote)
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
        description: "Set the system output volume: `level` 0–100, or `change` to go up or down \
                      by that many points (\"turn it up\" is about +10, \"mute\" is level 0).",
        // A system-wide action with no app of its own.
        app: crate::ALWAYS,
        read_only: false,
        params: || {
            json!({
                "type": "object",
                "properties": {
                    "level": {
                        "type": "integer", "minimum": 0, "maximum": 100,
                        "description": "Output volume, 0 (mute) to 100 (max)."
                    },
                    "change": {
                        "type": "integer", "minimum": -100, "maximum": 100,
                        "description": "Points to raise (positive) or lower (negative) the volume by."
                    }
                }
            })
        },
        run: |args| {
            let level = match (
                args.get("level").and_then(Value::as_i64),
                args.get("change").and_then(Value::as_i64),
            ) {
                (Some(level), _) => level,
                (None, Some(change)) => {
                    let now: i64 =
                        script::run("System Events", "output volume of (get volume settings)")?
                            .parse()
                            .map_err(|_| {
                                ScriptError::Failed("couldn't read the current volume".into())
                            })?;
                    now + change
                }
                (None, None) => {
                    return Err(ScriptError::Failed("give a level 0–100 or a change".into()))
                }
            }
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
        // the one non-negotiable property in this codebase. A tool that can
        // both read and act (now_playing vs play) must count as a write.
        let search = ACTIONS
            .iter()
            .find(|a| a.name == "ALOUD_FINDER_SEARCH")
            .unwrap();
        assert!(search.read_only, "FINDER_SEARCH only reads");
        for name in ["ALOUD_MUSIC", "ALOUD_SPOTIFY", "ALOUD_NOTES_CREATE"] {
            let action = ACTIONS.iter().find(|a| a.name == name).unwrap();
            assert!(
                !action.read_only,
                "{name} changes something — must be a write"
            );
        }
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

    /// "Save it to my desktop" had no tool, so the agent could summarise and
    /// then had nowhere to put the result.
    #[test]
    fn writing_a_file_lands_under_home_and_refuses_to_escape() {
        let name = format!(".aloud-test-write-{}/summary.md", std::process::id());
        let out = write_home_file(&name, "# Summary\n\nfirst line\n", false).unwrap();
        let written = std::path::PathBuf::from(out["wrote"].as_str().unwrap());
        assert!(written.exists(), "the file must actually be created");
        assert!(written.starts_with(std::env::var("HOME").unwrap()));

        // Append adds rather than replacing.
        write_home_file(&name, "second line\n", true).unwrap();
        let body = std::fs::read_to_string(&written).unwrap();
        assert!(
            body.contains("first line") && body.contains("second line"),
            "{body}"
        );

        // Replacing is the default, so a second summary does not accumulate.
        write_home_file(&name, "replaced\n", false).unwrap();
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "replaced\n");

        std::fs::remove_dir_all(written.parent().unwrap()).unwrap();

        for escape in ["../../etc/passwd", "/etc/passwd", "Desktop/../../../tmp/x"] {
            assert!(
                write_home_file(escape, "x", false).is_err(),
                "{escape:?} must be refused"
            );
        }
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
