//! Sorting a folder into subfolders, in one tool call.
//!
//! This is deliberately *not* a per-file move tool, which is what the reference
//! catalogues offer. The arithmetic rules it out: a real Downloads folder here
//! holds 73 files, while the loop allows one tool 3 calls per run
//! (`MAX_CALLS_PER_TOOL`) across 5 rounds. A move-one-file tool would sort three
//! of them and then be cut off. Even with the budget raised it would be 73
//! sequential model round-trips, each able to hallucinate a path.
//!
//! So the split is: **the model chooses the rules, Rust performs every move.**
//! The model sends "Images ← png, jpg; Installers ← dmg, pkg" and this executes
//! it over the whole folder atomically. The model still decides the scheme — it
//! can invent categories that suit what is actually there — but it never names
//! an individual file, so there is nothing per-file to get wrong.
//!
//! Everything here is reversible: [`plan`] is what the confirmation gate shows,
//! and every completed run writes a receipt that [`undo`] replays backwards.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::script::ScriptError;

/// Folders that may be organised, resolved against `$HOME`.
///
/// An allowlist rather than a blocklist: this moves real files, and the failure
/// mode of a missed blocklist entry is shuffling someone's system directory.
/// Desktop and Downloads are the two anyone actually means.
const ALLOWED: &[&str] = &["Desktop", "Downloads", "Documents"];

/// A single planned move. The gate renders these, so it is the file's own name
/// and its destination folder, not absolute paths nobody can read aloud.
#[derive(Debug, Clone)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// Resolves a spoken folder name to a real path, refusing anything outside the
/// allowlist. `~/Desktop`, `desktop` and `Desktop` all land in the same place.
fn resolve_folder(name: &str) -> Result<PathBuf, ScriptError> {
    let home =
        std::env::var("HOME").map_err(|_| ScriptError::Failed("no home directory".into()))?;
    let wanted = name
        .trim()
        .trim_start_matches("~/")
        .trim_end_matches('/')
        .trim();
    let matched = ALLOWED
        .iter()
        .find(|allowed| allowed.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| {
            ScriptError::Failed(format!(
                "{name:?} cannot be organised — only {} can",
                ALLOWED.join(", ")
            ))
        })?;
    Ok(PathBuf::from(home).join(matched))
}

/// Builds the move list for `folder` under `rules` (category → extensions).
///
/// Only loose files in the folder itself: subdirectories are left alone, so
/// running twice is harmless and an already-sorted folder plans nothing. Hidden
/// files are skipped — `.DS_Store` is not what anyone means by "organise".
pub fn plan(folder: &Path, rules: &BTreeMap<String, Vec<String>>) -> Vec<Move> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut moves = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // Directories are left where they are; recursing would fight the user's
        // own filing, and the categories below are already directories.
        if name.starts_with('.') || path.is_dir() {
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // First category claiming this extension wins, so overlapping rules are
        // resolved by the model's own ordering rather than arbitrarily.
        let category = rules.iter().find(|(_, exts)| {
            exts.iter()
                .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext))
        });
        let Some((category, _)) = category else {
            continue;
        };

        let destination = folder.join(category);
        moves.push(Move {
            from: path,
            to: destination.join(&name),
        });
    }
    moves.sort_by(|a, b| a.to.cmp(&b.to));
    moves
}

/// Performs `moves`, creating category folders as needed, and writes a receipt
/// so [`undo`] can put everything back.
///
/// A name already taken at the destination gets a numeric suffix rather than
/// overwriting: two files called `invoice.pdf` from different sources are two
/// different invoices, and silently destroying one is unforgivable in a tool
/// whose whole job is tidying.
pub fn apply(folder: &Path, moves: &[Move]) -> Result<Value, ScriptError> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut by_category: BTreeMap<String, usize> = BTreeMap::new();

    for mv in moves {
        let parent = mv
            .to
            .parent()
            .ok_or_else(|| ScriptError::Failed("a destination has no parent".into()))?;
        std::fs::create_dir_all(parent)
            .map_err(|e| ScriptError::Failed(format!("creating {}: {e}", parent.display())))?;

        let target = free_path(&mv.to);
        // `rename` rather than a copy+delete: it is atomic within a filesystem,
        // and both paths are inside the same folder by construction.
        std::fs::rename(&mv.from, &target).map_err(|e| {
            ScriptError::Failed(format!(
                "moving {}: {e}",
                mv.from.file_name().unwrap_or_default().to_string_lossy()
            ))
        })?;
        if let Some(category) = parent.file_name() {
            *by_category
                .entry(category.to_string_lossy().into_owned())
                .or_default() += 1;
        }
        done.push((target, mv.from.clone()));
    }

    write_receipt(folder, &done)?;
    Ok(json!({
        "organized": done.len(),
        "folder": folder.display().to_string(),
        "categories": by_category,
        "undo": "say \"undo that\" to put every file back",
    }))
}

/// Reverses the last organise run in `folder`.
///
/// The receipt is consumed whether or not every move reverses, so a partial
/// undo cannot be replayed twice — the second run would move files that are
/// already back, using stale paths.
pub fn undo(folder: &Path) -> Result<Value, ScriptError> {
    let path = receipt_path(folder);
    let contents = std::fs::read_to_string(&path)
        .map_err(|_| ScriptError::Failed("there is nothing to undo in that folder".into()))?;
    let _ = std::fs::remove_file(&path);

    let entries: Vec<(PathBuf, PathBuf)> = serde_json::from_str::<Vec<(String, String)>>(&contents)
        .map_err(|e| ScriptError::Failed(format!("unreadable undo record: {e}")))?
        .into_iter()
        .map(|(a, b)| (PathBuf::from(a), PathBuf::from(b)))
        .collect();

    let mut restored = 0usize;
    for (current, original) in entries.iter().rev() {
        if !current.exists() {
            continue;
        }
        if let Some(parent) = original.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::rename(current, free_path(original)).is_ok() {
            restored += 1;
        }
    }

    // The category folders are left behind only if the user put something else
    // in them; an empty one is ours and is swept up.
    if let Ok(dir) = std::fs::read_dir(folder) {
        for entry in dir.flatten() {
            if entry.path().is_dir() {
                let _ = std::fs::remove_dir(entry.path());
            }
        }
    }

    Ok(json!({ "restored": restored }))
}

/// `path`, or `path` with a numeric suffix when it is taken.
fn free_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = path.parent().unwrap_or(Path::new("."));
    for n in 2..1000 {
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

/// Where the undo receipt lives. Inside the organised folder and dot-prefixed,
/// so it travels with the folder and stays out of the listing — and out of
/// `plan`, which skips dotfiles.
fn receipt_path(folder: &Path) -> PathBuf {
    folder.join(".aloud-undo.json")
}

fn write_receipt(folder: &Path, done: &[(PathBuf, PathBuf)]) -> Result<(), ScriptError> {
    let pairs: Vec<(String, String)> = done
        .iter()
        .map(|(now, before)| (now.display().to_string(), before.display().to_string()))
        .collect();
    let json = serde_json::to_string(&pairs)
        .map_err(|e| ScriptError::Failed(format!("recording undo: {e}")))?;
    std::fs::write(receipt_path(folder), json)
        .map_err(|e| ScriptError::Failed(format!("recording undo: {e}")))
}

/// Reads the model's `rules` argument into category → extensions.
pub fn rules_from(args: &Value) -> BTreeMap<String, Vec<String>> {
    args.get("rules")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(category, exts)| {
                    let list = exts
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|e| e.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    (category.clone(), list)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The folder argument, resolved and allowlisted.
pub fn folder_from(args: &Value) -> Result<PathBuf, ScriptError> {
    let name = args
        .get("folder")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Downloads");
    resolve_folder(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([
            ("Images".into(), vec!["png".into(), "jpg".into()]),
            ("Installers".into(), vec![".dmg".into()]),
        ])
    }

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aloud-organize-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    /// Only the folders anyone means, and nothing outside home. This moves real
    /// files; a path escape here reorganises a system directory.
    #[test]
    fn only_allowlisted_folders_resolve() {
        assert!(resolve_folder("Desktop").is_ok());
        assert!(resolve_folder("downloads").is_ok());
        assert!(resolve_folder("~/Desktop").is_ok());

        assert!(resolve_folder("/etc").is_err());
        assert!(resolve_folder("../../etc").is_err());
        assert!(resolve_folder("Library").is_err());
        assert!(resolve_folder("~/.ssh").is_err());
    }

    #[test]
    fn plan_matches_extensions_and_leaves_everything_else() {
        let dir = scratch();
        touch(&dir, "a.png");
        touch(&dir, "b.JPG"); // case-insensitive
        touch(&dir, "c.dmg"); // rule written with a leading dot
        touch(&dir, "d.txt"); // no rule — untouched
        touch(&dir, ".hidden.png"); // dotfiles are not what "organise" means
        std::fs::create_dir_all(dir.join("Existing")).unwrap();

        let planned = plan(&dir, &rules());
        let names: Vec<String> = planned
            .iter()
            .map(|m| m.from.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert!(names.contains(&"a.png".to_string()));
        assert!(names.contains(&"b.JPG".to_string()));
        assert!(names.contains(&"c.dmg".to_string()));
        assert!(!names.contains(&"d.txt".to_string()), "unmatched must stay");
        assert!(!names.iter().any(|n| n.starts_with('.')));
        assert_eq!(planned.len(), 3);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The whole point of the design: one call sorts everything, so the 73-file
    /// case is not 73 tool calls.
    #[test]
    fn one_apply_sorts_the_whole_folder_and_undo_restores_it() {
        let dir = scratch();
        for i in 0..40 {
            touch(&dir, &format!("shot{i}.png"));
        }
        for i in 0..12 {
            touch(&dir, &format!("app{i}.dmg"));
        }
        touch(&dir, "notes.txt");

        let moves = plan(&dir, &rules());
        assert_eq!(moves.len(), 52);
        let out = apply(&dir, &moves).unwrap();
        assert_eq!(out["organized"], 52);

        assert!(dir.join("Images/shot0.png").exists());
        assert!(dir.join("Installers/app0.dmg").exists());
        assert!(
            dir.join("notes.txt").exists(),
            "unmatched file must not move"
        );

        let undone = undo(&dir).unwrap();
        assert_eq!(undone["restored"], 52);
        assert!(dir.join("shot0.png").exists(), "undo must restore in place");
        assert!(
            !dir.join("Images").exists(),
            "empty categories are swept up"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Two files of the same name from different places are two different
    /// files. Overwriting one while "tidying" would be silent data loss.
    #[test]
    fn a_name_collision_suffixes_rather_than_overwrites() {
        let dir = scratch();
        std::fs::create_dir_all(dir.join("Images")).unwrap();
        std::fs::write(dir.join("Images/photo.png"), b"original").unwrap();
        std::fs::write(dir.join("photo.png"), b"newcomer").unwrap();

        apply(&dir, &plan(&dir, &rules())).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.join("Images/photo.png")).unwrap(),
            "original",
            "the existing file must survive"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("Images/photo 2.png")).unwrap(),
            "newcomer"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Running twice must be harmless — the second pass sees only directories.
    #[test]
    fn organising_an_already_organised_folder_plans_nothing() {
        let dir = scratch();
        touch(&dir, "a.png");
        apply(&dir, &plan(&dir, &rules())).unwrap();
        assert!(plan(&dir, &rules()).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_without_a_previous_run_is_a_readable_error() {
        let dir = scratch();
        let err = undo(&dir).unwrap_err();
        assert!(err.to_string().contains("nothing to undo"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
