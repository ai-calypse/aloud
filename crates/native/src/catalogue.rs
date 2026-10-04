//! The curated actions. One table, one handler each.
//!
//! Curated, not a generic `RUN_APPLESCRIPT(source)` runner: the model would
//! write the code, and the confirmation gate would then show the user a script
//! they cannot judge. A gate you cannot read is not a gate. Each action here
//! gets a real label, a real body, and a `read_only` flag that means something.
//!
//! ponytail: a generic runner is the upgrade path if this table ever gets long
//! enough to be the bottleneck. It is a dozen rows; it is not.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::organize;
use crate::script::{self, ScriptError};
use crate::search;
use crate::{keys, media, notes, reminders};

/// Pulls a required string argument, or a readable error the model can recover
/// from — a missing field is the model's mistake, not a permission problem.
fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ScriptError> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ScriptError::Failed(format!("missing required argument {key:?}")))
}

/// Whether the app now in front is the one at `bundle` (canonical). Used to wait
/// out a launch, since `open` returns before the app is on screen.
///
/// Asked of System Events rather than the AX API: the AX read of the focused
/// application needs the calling process to be AX-trusted and fails outside the
/// signed app, which would make this loop always time out in tests and examples.
/// By bundle path, not process name: VS Code's process is "Code", so a name
/// never matched and every open waited out the full 10 s. Canonicalised, since
/// /Applications/Safari.app is a symlink into a Cryptex that System Events
/// reports by its real path.
fn frontmost_is(bundle: &Path) -> bool {
    let Ok(out) = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to POSIX path of (file of first \
             application process whose frontmost is true)",
        ])
        .output()
    else {
        return false;
    };
    std::fs::canonicalize(String::from_utf8_lossy(&out.stdout).trim())
        .is_ok_and(|front| front == bundle)
}

/// The app a tool acts in: the one it was told, else the one the user was in
/// when they spoke (`_front_app`, merged into the arguments by the caller). None
/// leaves today's System Events frontmost read, which with Aloud's panel up is
/// Aloud itself.
///
/// ponytail: an underscored key riding in the model's arguments; a `Context`
/// parameter on `Action::run` is the upgrade.
pub(crate) fn target_app(args: &Value) -> Option<&str> {
    ["app", "_front_app"].into_iter().find_map(|key| {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
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
fn open_one(name: &str) -> Result<(), ScriptError> {
    open_with(Some(&bundle_path(name)?), None)
}

/// `open [-a bundle] [target]`.
///
/// By path, never `open -b` or a URL scheme: Launch Services resolves a scheme
/// like `discord://` to Discord *Canary* when both are installed. `-a`
/// activates a running instance rather than starting a second, which is what
/// "switch to Safari" has to mean.
fn open_with(bundle: Option<&str>, target: Option<&str>) -> Result<(), ScriptError> {
    let what = target.or(bundle).unwrap_or_default();
    let mut open = std::process::Command::new("/usr/bin/open");
    if let Some(bundle) = bundle {
        open.args(["-a", bundle]);
    }
    let status = open
        .args(target)
        .status()
        .map_err(|e| ScriptError::Failed(format!("opening {what}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ScriptError::Failed(format!("could not open {what}")))
    }
}

/// Starts an app without bringing it forward, if it is not already running.
/// Only then: `open -g` on a running app sends it a reopen, which TextEdit
/// answers with a new window. Running is asked by bundle id, because by path a
/// symlinked bundle (Safari) reads as not running while it is.
fn launch_hidden(bundle: &str) -> Result<(), ScriptError> {
    let running = script::run_jxa(
        "System Events",
        "return Application(Application(ARG.bundle).id()).running();",
        &json!({ "bundle": bundle }),
    )?;
    if running == "true" {
        return Ok(());
    }
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-g", "-a", bundle])
        .status()
        .map_err(|e| ScriptError::Failed(format!("launching {bundle}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ScriptError::Failed(format!("could not launch {bundle}")))
    }
}

/// Where an installed app's bundle is, user-installed first: the first candidate
/// that exists wins, so a user-installed app outranks a system one of the same
/// name. A name that is not an app's own goes through [`real_app_name`].
fn bundle_path(name: &str) -> Result<String, ScriptError> {
    bundle_in_app_folders(name)
        .or_else(|| {
            real_app_name(name, &aloud_selection::installed_apps()).and_then(bundle_in_app_folders)
        })
        .ok_or_else(|| ScriptError::Failed(format!("no application named {name} is installed")))
}

/// The folders `installed_apps` scans, plus CoreServices for Finder.
fn bundle_in_app_folders(name: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        format!("/Applications/{name}.app"),
        format!("/Applications/Utilities/{name}.app"),
        format!("/System/Applications/{name}.app"),
        format!("/System/Applications/Utilities/{name}.app"),
        format!("{home}/Applications/{name}.app"),
        // Finder lives here, not in /Applications.
        format!("/System/Library/CoreServices/{name}.app"),
    ]
    .into_iter()
    .find(|p| Path::new(p).exists())
}

/// The installed app a spoken name means when it is not the app's own name: an
/// alias people say ("vs code" is Visual Studio Code), else the one installed
/// app whose name holds it as whole words ("canary"). Two such apps is None —
/// guessing between them opens the wrong one.
///
/// ponytail: a hand alias table and a word match; Spotlight's
/// `kMDItemAlternateNames` per bundle is the upgrade if the table grows.
fn real_app_name<'a>(
    said: &str,
    installed: &'a [aloud_selection::InstalledApp],
) -> Option<&'a str> {
    let said = said.trim().to_lowercase();
    let wanted = match said.as_str() {
        "vs code" | "vscode" | "code" => "visual studio code",
        "chrome" => "google chrome",
        "brave" => "brave browser",
        other => other,
    };
    if let Some(exact) = installed
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(wanted))
    {
        return Some(&exact.name);
    }
    let words = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let want = words(wanted);
    if want.is_empty() {
        return None;
    }
    let mut hits = installed
        .iter()
        .filter(|a| words(&a.name).windows(want.len()).any(|w| w == want));
    match (hits.next(), hits.next()) {
        (Some(one), None) => Some(&one.name),
        _ => None,
    }
}

/// Waits for `name` to actually reach the front.
///
/// `open` returns as soon as the launch is *requested*, not when the app is on
/// screen, so without this the agent's next step — a screen read or a click —
/// lands on the previous app. Polls rather than sleeping a flat second, so a
/// warm app costs ~50ms.
fn wait_until_frontmost(name: &str) {
    let Some(bundle) = bundle_path(name)
        .ok()
        .and_then(|b| std::fs::canonicalize(b).ok())
    else {
        return;
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if frontmost_is(&bundle) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// How long Photo Booth may keep Take Photo greyed out. A calibration knob: on
/// a 2026 MacBook the item went grey 1.0s after the press and came back at 5.2s,
/// once the countdown and shutter were done. Raise it if a slower camera times out.
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// Waits until Photo Booth's Take Photo can be pressed, polled in one osascript
/// rather than one per poll. `after_shot`: the item stays enabled for about a
/// second after a press before the countdown greys it, so it must be seen grey
/// first — else the "ready" read is the stale one from before the press. False
/// on timeout.
fn wait_ready(after_shot: bool) -> Result<bool, ScriptError> {
    let out = script::run_jxa(
        "System Events",
        r#"const proc = Application("System Events").processes.byName("Photo Booth");
        const find = () => {
            for (const bar of proc.menuBars[0].menuBarItems()) {
                const hits = bar.menus[0].menuItems.whose({ name: { _beginsWith: "Take" } })();
                if (hits.length) return hits[0];
            }
            return null;
        };
        const until = Date.now() + ARG.timeout_ms;
        let item = null, greyed = !ARG.after_shot;
        while (Date.now() < until) {
            try {
                item = item || find();
                if (!item || !item.enabled()) greyed = true;
                else if (greyed) return "ready";
            } catch (e) { item = null; }
            delay(0.1);
        }
        return "timeout";"#,
        &json!({ "after_shot": after_shot, "timeout_ms": READY_TIMEOUT.as_millis() as u64 }),
    )?;
    Ok(out == "ready")
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

/// The URL scheme `target` starts with, or None for a path or a bare name.
fn url_scheme(target: &str) -> Option<&str> {
    let (scheme, _) = target.split_once(':')?;
    let mut chars = scheme.chars();
    let valid = chars.next()?.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    valid.then_some(scheme)
}

/// A scheme other than a file or a web link — `slack:`, `tel:`, an app's own —
/// which opens like the rest but is reported, so the permission tier can make
/// it ask every time.
fn flagged_scheme(target: &str) -> Option<&str> {
    url_scheme(target).filter(|s| {
        !["http", "https", "file"]
            .iter()
            .any(|ok| s.eq_ignore_ascii_case(ok))
    })
}

/// The file or folder a bare name means, best first: one path, several when the
/// best are tied (the model asks between them), or none.
///
/// `document` is the front window's file, else the folder of the `project` an
/// editor's title names (see [`editor_project`]). While the user is in a repo,
/// "open components" means that repo's components, so its root — walked up to
/// the `.git`, never home or above — wins when it has that name, then an exact
/// match inside it. Only then home, ranked by [`best_named`].
pub(crate) fn resolve_named(
    name: &str,
    document: Option<&Path>,
    project: Option<&str>,
) -> Vec<PathBuf> {
    let Ok(home) = home_dir() else {
        return Vec::new();
    };
    let near = document
        .map(Path::to_path_buf)
        .or_else(|| project.and_then(|p| project_root(p, &home, spotlight_folders)));
    resolve_named_in(name, near.as_deref(), &home, spotlight_named)
}

/// The project an editor window shows, from its title. Cursor titles a window
/// "main.rs — aloud-tauri" and gives no `AXDocument` (checked), so the title's
/// last part is the only sign of which repo is on screen.
///
/// ponytail: four editors by name, each assumed to end its title with the
/// project; a per-editor title format is the upgrade when one doesn't.
pub fn editor_project<'a>(app: &str, title: &'a str) -> Option<&'a str> {
    const EDITORS: &[&str] = &["Cursor", "Visual Studio Code", "Zed", "Xcode"];
    if !EDITORS.iter().any(|e| e.eq_ignore_ascii_case(app)) {
        return None;
    }
    title
        .rsplit(" — ")
        .next()
        .map(str::trim)
        .filter(|p| !p.is_empty())
}

/// The repo under home named `project`, shallowest first: a second clone tucked
/// deeper — a backup, a vendored copy — is the less likely one to be open.
fn project_root(
    project: &str,
    home: &Path,
    find: impl Fn(&Path, &str) -> Vec<PathBuf>,
) -> Option<PathBuf> {
    find(home, project)
        .into_iter()
        .filter(|p| name_tier(p, project) == 0 && p.join(".git").exists())
        .min_by_key(|p| p.components().count())
}

/// [`resolve_named`] with home and the search passed in, so a test can run it
/// over a scratch folder Spotlight has not indexed.
fn resolve_named_in(
    name: &str,
    near: Option<&Path>,
    home: &Path,
    find: impl Fn(&Path, &str) -> Vec<PathBuf>,
) -> Vec<PathBuf> {
    let root = near.and_then(|near| {
        near.ancestors()
            .take_while(|dir| dir.starts_with(home) && *dir != home)
            .find(|dir| dir.join(".git").exists())
    });
    if let Some(root) = root {
        if root
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
        {
            return vec![root.to_path_buf()];
        }
        let inside: Vec<PathBuf> = find(root, name)
            .into_iter()
            .filter(|p| name_tier(p, name) < 2)
            .collect();
        if !inside.is_empty() {
            return best_named(inside, name);
        }
    }
    best_named(find(home, name), name)
}

/// How well a path's name matches: 0 the name itself, 1 the name without its
/// extension ("budget" is `budget.numbers`), 2 only a prefix. A folder called
/// `components` must outrank `components.css`, or "open components" opens CSS.
fn name_tier(path: &Path, name: &str) -> u8 {
    let is = |part: Option<&std::ffi::OsStr>| {
        part.is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
    };
    if is(path.file_name()) {
        0
    } else if is(path.file_stem()) {
        1
    } else {
        2
    }
}

/// The hits tied for best: by [`name_tier`], then the day last modified, then
/// the shallowest. By the day on purpose — a folder touched an hour after
/// another is no reason to open it without asking.
fn best_named(hits: Vec<PathBuf>, name: &str) -> Vec<PathBuf> {
    let rank = |path: &PathBuf| {
        let day = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() / 86_400);
        (
            name_tier(path, name),
            std::cmp::Reverse(day),
            path.components().count(),
        )
    };
    let mut ranked: Vec<_> = hits.into_iter().map(|p| (rank(&p), p)).collect();
    ranked.sort();
    let Some(top) = ranked.first().map(|(r, _)| *r) else {
        return Vec::new();
    };
    ranked
        .into_iter()
        .take_while(|(r, _)| *r == top)
        .map(|(_, p)| p)
        .take(5)
        .collect()
}

/// Files and folders under `root` whose name is `name` or starts with it,
/// without the build output and caches that match any common name.
fn spotlight_named(root: &Path, name: &str) -> Vec<PathBuf> {
    spotlight(
        root,
        &format!("kMDItemFSName == '{}*'c", search::escape_mdfind(name)),
    )
}

/// Folders under `root` named exactly `name`.
fn spotlight_folders(root: &Path, name: &str) -> Vec<PathBuf> {
    spotlight(
        root,
        &format!(
            "kMDItemFSName == '{}'c && kMDItemContentType == 'public.folder'",
            search::escape_mdfind(name)
        ),
    )
}

fn spotlight(root: &Path, query: &str) -> Vec<PathBuf> {
    let Ok(out) = std::process::Command::new("mdfind")
        .arg("-onlyin")
        .arg(root)
        .arg(query)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !search::is_noise(l))
        .map(PathBuf::from)
        .collect()
}

/// The Notes scripts work in the background: activating Notes and showing the
/// note pulled the user out of the app they were dictating into. The note's id
/// rides back so a later "Show" can open exactly that note.
const NOTES_CREATE_JS: &str = r#"const Notes = Application("Notes");
    const note = Notes.make({ new: "note", withProperties: { body: ARG.body } });
    return JSON.stringify({ created: true, title: note.name(), id: note.id() });"#;

/// Appending rewrites the whole body, and attachments are not in the HTML a
/// script reads, so a note with any is refused rather than silently stripped of
/// them. See [`NOTES_CREATE_JS`] for why nothing here brings Notes forward.
const NOTES_APPEND_JS: &str = r#"const Notes = Application("Notes");
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
        return JSON.stringify({ created: true, title: note.name(), id: note.id() });
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
    return JSON.stringify({ appended: true, title: note.name(), id: note.id() });"#;

/// The one Notes script that brings Notes forward: the user pressed Show.
const NOTES_SHOW_JS: &str = r#"const Notes = Application("Notes");
    Notes.show(Notes.notes.byId(ARG.id));
    Notes.activate();"#;

/// Brings back what a finished write made, from `show_target`: the note by its
/// id, a saved file revealed in Finder, anything else opened in the app named.
pub fn show(app: &str, target: &str) -> Result<(), ScriptError> {
    match app {
        "Notes" => script::run_jxa("Notes", NOTES_SHOW_JS, &json!({ "id": target })).map(drop),
        "Finder" => {
            let revealed = std::process::Command::new("/usr/bin/open")
                .args(["-R", target])
                .status()
                .is_ok_and(|s| s.success());
            revealed
                .then_some(())
                .ok_or_else(|| ScriptError::Failed(format!("could not reveal {target}")))
        }
        _ => open_with(Some(&bundle_path(app)?), Some(target)),
    }
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
    /// Offered a `times` argument and run that many times by `lib::execute`, so
    /// "take three photos" is one call rather than three the model may not make.
    pub repeatable: bool,
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
                      'folder' → kind=folder. \
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
        repeatable: false,
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
                        "enum": ["screenshot", "image", "document", "folder", "any"],
                        "description": "File type. pictures/photos/images→image, \
                                        screenshots→screenshot, docs/PDFs→\
                                        document, folders→folder, files→any."
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
        repeatable: true,
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
        repeatable: true,
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
        repeatable: false,
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
        description: "Create a new note in the Notes app. Write the content yourself \
                      into `body` as markdown (lines, `- ` bullets, `1.` steps, `## ` headings): \
                      for \"make a shopping list for a cake and put it in a note\", body is the \
                      actual list. Plain text, no emoji. Omit `body` for an empty note.",
        app: "Notes",
        read_only: false,
        repeatable: false,
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
            let out = script::run_jxa(
                "Notes",
                NOTES_CREATE_JS,
                &json!({ "body": notes::note_body(title, body) }),
            )?;
            Ok(serde_json::from_str(&out).unwrap_or_else(|_| json!({ "created": true })))
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
        repeatable: false,
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
            let out = script::run_jxa(
                "Notes",
                NOTES_APPEND_JS,
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
        repeatable: true,
        params: || json!({ "type": "object", "properties": {} }),
        run: |_args| {
            open_one("Photo Booth")?;
            wait_until_frontmost("Photo Booth");
            // A press while Take Photo is greyed out (a countdown under way) is ignored. A timeout
            // here still tries: the press below falls back to the window's button.
            wait_ready(false)?;
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
            // Back only once the shutter has fired, so a repeat's next press lands on a ready
            // camera and the confirmation never beats the photo.
            if !wait_ready(true)? {
                return Err(ScriptError::Failed(format!(
                    "Photo Booth's Take Photo did not come back within {}s; the photo may not \
                     have been taken",
                    READY_TIMEOUT.as_secs()
                )));
            }
            Ok(json!({ "took_photo": true, "via": how }))
        },
    },
    Action {
        name: "ALOUD_DISCORD_MESSAGE",
        description: "Send a Discord message to a person or channel by name: opens Discord's \
                      quick switcher, goes to `to`, and sends `text`. Write the message yourself.",
        app: "Discord",
        read_only: false,
        repeatable: false,
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
        name: "ALOUD_PRESS_KEYS",
        description: "Press a keyboard shortcut — anything the Mac or an app does from the \
                      keyboard that no named tool covers: cmd+shift+5 opens the screenshot and \
                      screen-recording toolbar (to record the screen, press it and tell the user to \
                      click Record), cmd+shift+4 captures an area, ctrl+cmd+q locks the \
                      screen, cmd+space opens Spotlight, cmd+t a new tab. `keys` is one chord like \
                      \"cmd+shift+5\". `app` brings that app to the front first; omit it for a \
                      system-wide shortcut or the app already in front. Never say you have no tool \
                      for something the Mac does with keys.",
        app: crate::ALWAYS,
        read_only: false,
        repeatable: true,
        params: || {
            json!({
                "type": "object",
                "required": ["keys"],
                "properties": {
                    "keys": { "type": "string", "description": "One chord, e.g. \"cmd+shift+5\", \"ctrl+cmd+q\", \"escape\"." },
                    "app": { "type": "string", "description": "The app to press it in, if not the one in front." }
                }
            })
        },
        run: |args| {
            let spec = required_str(args, "keys")?;
            let chord = keys::parse(spec).map_err(ScriptError::Failed)?;
            // Keys land in whatever is in front, which with Aloud's panel up is
            // Aloud, so the app the user was in comes forward first.
            if let Some(app) = target_app(args) {
                open_one(app)?;
                wait_until_frontmost(app);
            }
            aloud_paste::press_chord(&chord.modifiers, chord.key)
                .map_err(|e| ScriptError::Failed(format!("{e:#}")))?;
            Ok(json!({ "pressed": spec }))
        },
    },
    Action {
        name: "ALOUD_MENU",
        description: "Run a command from an app's menu bar by name — anything in its File, Edit, \
                      View… menus that no named tool covers: \"New Window\", \"Export as PDF\", \
                      \"Show Sidebar\", \"Zoom In\". Give `item` (its name) or `path` [\"File\", \
                      \"Export as PDF…\"]. Unsure of the name? Call with `list: true` first to see \
                      the app's menu items. `app` defaults to the app in front.",
        app: crate::ALWAYS,
        read_only: false,
        repeatable: true,
        params: || {
            json!({
                "type": "object",
                "properties": {
                    "item": { "type": "string", "description": "The menu item's name." },
                    "path": { "type": "array", "items": { "type": "string" }, "description": "Menu, then item (then submenu item)." },
                    "list": { "type": "boolean", "description": "List the app's menu items instead of clicking one." },
                    "app": { "type": "string", "description": "The app, if not the one in front." }
                }
            })
        },
        run: |args| {
            let app = target_app(args);
            // An AX menu press reaches a background app without bringing it
            // forward (TextEdit's File > New opened a document while VLC stayed
            // in front), so the app is only launched, hidden, when not running.
            let bundle = app.map(bundle_path).transpose()?;
            if let Some(bundle) = &bundle {
                launch_hidden(bundle)?;
            }
            // Names are fetched a menu at a time (one Apple Event per menu, not
            // per item), and submenus only when the top level has no match: a
            // full walk of a big app's menus took seconds.
            let out = script::run_jxa(
                "System Events",
                r#"const se = Application("System Events");
                let proc = null;
                if (ARG.bundle) {
                    // By bundle id, not name: VS Code's process is "Code". Polled, since a
                    // hidden launch has no menu bar for a moment.
                    const id = Application(ARG.bundle).id();
                    for (let i = 0; i < 50 && !proc; i++) {
                        const hits = se.processes.whose({ bundleIdentifier: id })();
                        if (hits.length && hits[0].menuBars.length) proc = hits[0];
                        else delay(0.1);
                    }
                    if (!proc) throw new Error(ARG.app + " did not start in time");
                } else proc = se.processes.whose({ frontmost: true })[0];
                const norm = (s) => String(s || "").toLowerCase().replace(/[….]+$/u, "").trim();
                const tops = proc.menuBars[0].menuBarItems;
                const topNames = tops.name().slice(1); // the Apple menu is the system's, not the app's
                const entries = [];
                topNames.forEach((top, t) => {
                    const items = tops[t + 1].menus[0].menuItems;
                    items.name().forEach((name, i) => { if (name) entries.push({ path: [top, name], item: items[i] }); });
                });
                const findIn = (list) => {
                    if (ARG.path && ARG.path.length) {
                        const want = ARG.path.map(norm);
                        return list.find((e) => e.path.length === want.length && e.path.every((p, i) => norm(p) === want[i]));
                    }
                    const want = norm(ARG.item);
                    return list.find((e) => norm(e.path[e.path.length - 1]) === want)
                        || list.find((e) => norm(e.path[e.path.length - 1]).includes(want));
                };
                let hit = ARG.list ? null : findIn(entries);
                if (!hit && (ARG.list || (ARG.path && ARG.path.length > 2) || ARG.item)) {
                    const deeper = [];
                    for (const e of entries) {
                        try {
                            const sub = e.item.menus[0].menuItems;
                            sub.name().forEach((name, i) => { if (name) deeper.push({ path: [...e.path, name], item: sub[i] }); });
                        } catch (x) {}
                    }
                    if (ARG.list) return JSON.stringify({ items: entries.concat(deeper).map((e) => e.path.join(" > ")).slice(0, 400) });
                    hit = findIn(deeper);
                }
                if (!hit) throw new Error("no menu item " + JSON.stringify(ARG.path || ARG.item) + " in " + proc.name() + "; call with list to see them");
                if (!hit.item.enabled()) throw new Error("“" + hit.path.join(" > ") + "” is greyed out right now");
                hit.item.click();
                return JSON.stringify({ clicked: hit.path.join(" > "), app: proc.name() });"#,
                &json!({
                    "app": app,
                    "bundle": bundle,
                    "item": args.get("item").and_then(Value::as_str),
                    "path": args.get("path"),
                    "list": args.get("list").and_then(Value::as_bool).unwrap_or(false),
                }),
            )?;
            Ok(serde_json::from_str(&out).unwrap_or_else(|_| json!({ "done": out })))
        },
    },
    Action {
        name: "ALOUD_MESSAGES_SEND",
        description: "Send an iMessage or SMS to a phone number or email handle. \
                      The `to` field must be a full handle, never a name — resolve \
                      names to a handle first, or ask.",
        app: "Messages",
        read_only: false,
        repeatable: false,
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
             the Dock on screen to do it. To open a file, folder or link IN an app \
             (\"open components in Cursor\"), pass it as `open`.",
        app: crate::ALWAYS,
        read_only: false,
        repeatable: false,
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
                    },
                    "open": {
                        "type": "string",
                        "description": "What to open in the last app: a path, \
                                        ~/path or URL, or just the file or folder's \
                                        name as said (\"components\") to look it up.",
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

            let target = match args
                .get("open")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                None => None,
                Some(link) if url_scheme(link).is_some() => Some(link.to_string()),
                Some(path) if path.contains('/') => {
                    let path = path.trim_start_matches("~/");
                    Some(if path.starts_with('/') {
                        path.to_string()
                    } else {
                        home_dir()?.join(path).display().to_string()
                    })
                }
                Some(name) => {
                    let front = |key| args.get(key).and_then(Value::as_str);
                    let document = front("_front_document").map(Path::new);
                    match resolve_named(name, document, front("_front_project")).as_slice() {
                        [] => {
                            return Err(ScriptError::Failed(format!(
                                "found no file or folder named {name:?}"
                            )))
                        }
                        [one] => Some(one.display().to_string()),
                        several => {
                            let candidates: Vec<String> =
                                several.iter().map(|p| p.display().to_string()).collect();
                            return Ok(json!({
                                "opened": false,
                                "candidates": candidates,
                                "note": "ask which, with these as options",
                            }));
                        }
                    }
                }
            };

            let mut apps = Vec::new();
            let mut missing = Vec::new();
            for name in &names {
                match bundle_path(name) {
                    Ok(bundle) => apps.push((name.clone(), bundle)),
                    Err(_) => missing.push(name.clone()),
                }
            }
            if apps.is_empty() {
                return Err(ScriptError::Failed(format!(
                    "none of those apps are installed: {}",
                    missing.join(", ")
                )));
            }
            // The target goes to the last app, the one left in front.
            let last = apps.len() - 1;
            let mut opened = Vec::new();
            for (i, (name, bundle)) in apps.into_iter().enumerate() {
                let with = target.as_deref().filter(|_| i == last);
                match open_with(Some(&bundle), with) {
                    Ok(()) => opened.push(name),
                    Err(e) if with.is_some() => return Err(e),
                    Err(_) => missing.push(name),
                }
            }
            // The last one opened is the one left in front, so a follow-up
            // screen read targets something predictable.
            if let Some(last) = opened.last() {
                wait_until_frontmost(last);
            }
            let mut result = json!({ "opened": opened, "not_installed": missing });
            if let Some(target) = &target {
                result["target"] = json!(target);
                if let Some(scheme) = flagged_scheme(target) {
                    result["scheme"] = json!(scheme);
                }
            }
            Ok(result)
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
        repeatable: false,
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
        repeatable: false,
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
                let bundle = match with {
                    "cursor" => Some(bundle_path("Cursor")?),
                    "vscode" => Some(bundle_path("Visual Studio Code")?),
                    _ => None,
                };
                wrote["opened"] = json!(open_with(bundle.as_deref(), Some(&file)).is_ok());
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
        repeatable: false,
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
        repeatable: false,
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
        repeatable: false,
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
            let app = target_app(args)
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
        repeatable: false,
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
        repeatable: false,
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
        repeatable: false,
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
        repeatable: false,
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
        repeatable: false,
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

    /// "Make a note" yanked the user out of the app they were dictating into,
    /// because both scripts ended by activating Notes.
    #[test]
    fn notes_scripts_never_activate_notes() {
        for script in [NOTES_CREATE_JS, NOTES_APPEND_JS] {
            assert!(!script.contains("activate"), "{script}");
            assert!(!script.contains(".show("), "{script}");
            assert!(script.contains("id: note.id()"), "the id must come back");
        }
    }

    /// VS Code's bundle is "Visual Studio Code", which nobody says; "chrome"
    /// must not pick Canary just because it also has the word.
    #[test]
    fn vs_code_and_chrome_resolve_to_their_real_bundles() {
        let installed: Vec<aloud_selection::InstalledApp> = [
            "Visual Studio Code",
            "Xcode",
            "Google Chrome",
            "Google Chrome Canary",
            "Brave Browser",
            "Android Studio",
            "Microsoft Visual Studio",
        ]
        .into_iter()
        .map(|name| aloud_selection::InstalledApp {
            name: name.into(),
            bundle_id: None,
            icon: None,
        })
        .collect();
        let real = |said| real_app_name(said, &installed);
        for said in ["vs code", "VSCode", "code"] {
            assert_eq!(real(said), Some("Visual Studio Code"), "{said}");
        }
        assert_eq!(real("chrome"), Some("Google Chrome"));
        assert_eq!(real("brave"), Some("Brave Browser"));
        assert_eq!(real("canary"), Some("Google Chrome Canary"));
        // Two apps share the word: guessing would open the wrong one.
        assert_eq!(real("studio"), None);
        // Whole words only: "code" is not inside "Xcode" (the alias aside).
        assert_eq!(real("xco"), None);
        assert_eq!(real("  "), None);
    }

    /// MENU, PRESS_KEYS and WINDOW_ARRANGE with no `app` hit Aloud's own
    /// panel, the thing in front while the user speaks.
    #[test]
    fn a_tool_without_an_app_targets_the_app_the_user_was_in() {
        let front = json!({ "keys": "cmd+t", "_front_app": "Safari" });
        assert_eq!(target_app(&front), Some("Safari"));
        let named = json!({ "app": "TextEdit", "_front_app": "Safari" });
        assert_eq!(target_app(&named), Some("TextEdit"), "a named app wins");
        let blank = json!({ "app": " ", "_front_app": "Safari" });
        assert_eq!(target_app(&blank), Some("Safari"));
        assert_eq!(target_app(&json!({ "keys": "cmd+t" })), None);
    }

    /// Only these open without a mark; anything else (`slack:`, `tel:`) is
    /// reported so the permission tier can make it ask every time.
    #[test]
    fn only_files_and_web_links_open_without_a_scheme_flag() {
        for plain in [
            "https://example.com",
            "HTTP://example.com",
            "file:///Users/me/notes.md",
            "/Users/me/proj",
            "~/proj/src",
            "components",
        ] {
            assert_eq!(flagged_scheme(plain), None, "{plain}");
        }
        assert_eq!(flagged_scheme("slack://channel?id=1"), Some("slack"));
        assert_eq!(flagged_scheme("mailto:a@b.c"), Some("mailto"));
        assert_eq!(
            flagged_scheme("x-apple.systempreferences:com.apple"),
            Some("x-apple.systempreferences")
        );
        assert_eq!(url_scheme("components"), None, "a bare name is no link");
    }

    /// A scratch home with real folders: Spotlight never indexes these, so the
    /// search is a plain walk, prefix-matching like the real one.
    fn scratch_home(tag: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("aloud-named-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    fn walk(root: &Path, name: &str) -> Vec<PathBuf> {
        let mut hits = Vec::new();
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            let path = entry.path();
            let file = entry.file_name().to_string_lossy().to_lowercase();
            if file.starts_with(&name.to_lowercase()) {
                hits.push(path.clone());
            }
            if path.is_dir() {
                hits.extend(walk(&path, name));
            }
        }
        hits
    }

    /// "Open components in Cursor" while in a repo must open that repo's
    /// components, not a shallower one elsewhere in home.
    #[test]
    fn a_bare_name_prefers_the_project_on_screen() {
        let home = scratch_home("project");
        for dir in ["old/components", "app/.git", "app/src/components", ".git"] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        let doc = home.join("app/src/main.ts");
        std::fs::write(&doc, "").unwrap();

        let named = |name, near: Option<&Path>| resolve_named_in(name, near, &home, walk);
        assert_eq!(
            named("components", Some(&doc)),
            vec![home.join("app/src/components")]
        );
        // Off the project, home's shallowest wins.
        assert_eq!(named("components", None), vec![home.join("old/components")]);
        // A repo named what was said is itself the answer.
        assert_eq!(named("APP", Some(&doc)), vec![home.join("app")]);
        // Home's own .git (a dotfiles repo) is no project, so home itself is
        // never the answer.
        let home_name = home.file_name().unwrap().to_string_lossy().into_owned();
        assert!(named(home_name.as_str(), Some(&home.join("old/components"))).is_empty());

        std::fs::remove_dir_all(&home).unwrap();
    }

    /// Cursor gives no AXDocument, only "main.rs — aloud-tauri": that title's
    /// project is where "open components" looks before the rest of home.
    #[test]
    fn an_editor_title_names_the_project_to_search_first() {
        assert_eq!(
            editor_project("Cursor", "main.rs — aloud-tauri"),
            Some("aloud-tauri")
        );
        assert_eq!(
            editor_project("Visual Studio Code", "● lib.rs — src — aloud-tauri"),
            Some("aloud-tauri")
        );
        assert_eq!(editor_project("Safari", "Docs — Example"), None);

        let home = scratch_home("editor");
        for dir in [
            "old/components",
            "notes/aloud-tauri",
            "Dev/aloud-tauri/.git",
            "Dev/aloud-tauri/apps/src/components",
            "backup/deeper/aloud-tauri/.git",
        ] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        // Not the same-depth folder that is no repo, nor the deeper clone.
        let root = project_root("aloud-tauri", &home, walk);
        assert_eq!(root, Some(home.join("Dev/aloud-tauri")));
        assert_eq!(
            resolve_named_in("components", root.as_deref(), &home, walk),
            vec![home.join("Dev/aloud-tauri/apps/src/components")]
        );
        assert_eq!(project_root("nowhere", &home, walk), None);

        std::fs::remove_dir_all(&home).unwrap();
    }

    /// Two folders equally likely to be "components" must come back as a
    /// question, not a coin toss — and neither `components.css` nor a prefix
    /// match ties the exact name.
    #[test]
    fn two_equally_good_matches_come_back_for_a_question() {
        let home = scratch_home("tie");
        for dir in ["a/components", "b/components", "c", "components-old"] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }
        std::fs::write(home.join("c/components.css"), "").unwrap();
        let mut hits = resolve_named_in("components", None, &home, walk);
        hits.sort();
        assert_eq!(
            hits,
            vec![home.join("a/components"), home.join("b/components")]
        );
        std::fs::remove_dir_all(&home).unwrap();
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
