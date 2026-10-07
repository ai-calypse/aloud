//! The journal: thoughts captured by voice or typed, one markdown file each
//! under `<app-data>/Aloud/Journal`. Plain files, so they open in any editor
//! and outlive the app; named by when they were first written, so the folder
//! sorts the way the timeline does.
//!
//! Every path the webview names is checked to be an entry directly in the
//! journal folder before anything is written or removed — the same rule
//! `save_quick` holds for the notch's quick notes.

use crate::{file_mtime, Note, SearchResult};
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// UTC, like every other note's name (`write_note`); the view shows local time.
const FILE_FORMAT: &str = "%Y-%m-%d %H%M%S";
/// "2026-07-14 093012" — the timestamp prefix, in characters.
const STAMP_LEN: usize = 17;
/// How much of the first line names an entry in the list.
const TITLE_CHARS: usize = 80;

/// `<app-data>/Aloud/Journal`, created on first save.
pub fn directory() -> Result<PathBuf> {
    Ok(crate::data_dir()?.join("Journal"))
}

/// Every entry, newest first.
pub fn entries(dir: &Path) -> Vec<Note> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<Note> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_markdown(p))
        .filter_map(|p| entry_from(&p))
        .collect();
    // Name breaks a same-second tie, so the order is stable between reads.
    entries.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.path.cmp(&a.path)));
    entries
}

/// Keyword search over `entries`, ranked as notes are: every term must
/// appear, and hits in the first line outrank the rest.
pub fn search(query: &str, entries: &[Note]) -> Vec<SearchResult> {
    crate::search(query, entries)
}

fn entry_from(path: &Path) -> Option<Note> {
    let content = fs::read_to_string(path).ok()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let date = date_of(&stem).or_else(|| file_mtime(path)).unwrap_or(0);
    Some(Note {
        title: title_of(&content),
        path: path.to_path_buf(),
        date,
        content,
    })
}

/// When an entry was first written, from its file name.
fn date_of(stem: &str) -> Option<i64> {
    let prefix: String = stem.chars().take(STAMP_LEN).collect();
    NaiveDateTime::parse_from_str(&prefix, FILE_FORMAT)
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}

/// The first line that says anything, without a heading's `#`s, cut to fit a
/// list row. An entry is a thought, not a document, so it has no title of its
/// own to find.
pub fn title_of(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= TITLE_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// Writes a new entry, or replaces the one at `path`. Returns where it went.
///
/// `path` comes from the webview, so it must name an existing entry directly
/// in `dir` — never a meeting note, never a path that climbs out of it.
/// Replacing goes through a temp file and a rename: this is the user's own
/// writing, and a crash mid-write must not leave it truncated.
pub fn save(dir: &Path, path: Option<&Path>, text: &str, at: DateTime<Utc>) -> Result<PathBuf> {
    ensure!(!text.trim().is_empty(), "an empty entry is not kept");
    let target = match path {
        Some(path) => {
            ensure!(
                is_entry(dir, path) && path.is_file(),
                "{} is not a journal entry",
                path.display()
            );
            path.to_path_buf()
        }
        None => {
            fs::create_dir_all(dir)
                .with_context(|| format!("creating the journal at {}", dir.display()))?;
            fresh_path(dir, at)
        }
    };
    write_atomic(&target, text)?;
    Ok(target)
}

/// Removes an entry. Same rule as [`save`] for the path it is handed.
pub fn delete(dir: &Path, path: &Path) -> Result<()> {
    ensure!(
        is_entry(dir, path),
        "{} is not a journal entry",
        path.display()
    );
    fs::remove_file(path).with_context(|| format!("removing {}", path.display()))
}

/// A name for an entry written at `at` that nothing has yet. Two thoughts in
/// one second are possible (a capture finishing as one is typed), so the
/// second gets a counter rather than overwriting the first.
fn fresh_path(dir: &Path, at: DateTime<Utc>) -> PathBuf {
    let stem = at.format(FILE_FORMAT).to_string();
    let mut path = dir.join(format!("{stem}.md"));
    let mut counter = 2;
    while path.exists() {
        path = dir.join(format!("{stem} {counter}.md"));
        counter += 1;
    }
    path
}

/// Directly inside `dir` and markdown. Compared as written, not resolved:
/// `dir/../x.md` and `dir/./x.md` both have a parent that is not `dir`, so a
/// path dressed up to look inside is refused rather than normalised into it.
fn is_entry(dir: &Path, path: &Path) -> bool {
    path.parent() == Some(dir) && is_markdown(path)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let temp = path.with_extension("md.tmp");
    fs::write(&temp, text).with_context(|| format!("writing {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("saving {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn entries_are_named_by_time_and_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let first = save(dir.path(), None, "first thought", at(1_000)).unwrap();
        let second = save(dir.path(), None, "second thought", at(2_000)).unwrap();
        let listed = entries(dir.path());
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].path, second);
        assert_eq!(listed[1].path, first);
        assert_eq!(
            listed[1].date, 1_000,
            "the date round-trips through the name"
        );
    }

    /// Two thoughts landing in one second (a capture finishing while one is
    /// typed) must both be kept.
    #[test]
    fn two_entries_in_the_same_second_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let a = save(dir.path(), None, "one", at(5)).unwrap();
        let b = save(dir.path(), None, "two", at(5)).unwrap();
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(&a).unwrap(), "one");
        assert_eq!(fs::read_to_string(&b).unwrap(), "two");
    }

    /// The webview names the file to overwrite or remove; it must not be able
    /// to name anything but an entry in the journal folder.
    #[test]
    fn only_entries_in_the_journal_can_be_replaced_or_removed() {
        let root = tempfile::tempdir().unwrap();
        let journal = root.path().join("Journal");
        let entry = save(&journal, None, "mine", at(0)).unwrap();

        let outside = root.path().join("meeting.md");
        fs::write(&outside, "keep me").unwrap();
        assert!(save(&journal, Some(&outside), "x", at(0)).is_err());
        assert!(delete(&journal, &outside).is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "keep me");

        let climbing = journal.join("..").join("meeting.md");
        assert!(save(&journal, Some(&climbing), "x", at(0)).is_err());
        assert!(delete(&journal, &climbing).is_err());
        assert!(outside.exists());

        let not_markdown = journal.join("settings.json");
        fs::write(&not_markdown, "{}").unwrap();
        assert!(delete(&journal, &not_markdown).is_err());

        assert!(
            save(
                &journal,
                Some(&journal.join("2001-01-01 000000.md")),
                "x",
                at(0)
            )
            .is_err(),
            "replacing needs an entry that exists"
        );

        let edited = save(&journal, Some(&entry), "mine, edited", at(9)).unwrap();
        assert_eq!(edited, entry, "an edit keeps the entry's name and date");
        assert_eq!(fs::read_to_string(&entry).unwrap(), "mine, edited");
        delete(&journal, &entry).unwrap();
        assert!(entries(&journal).is_empty());
    }

    #[test]
    fn an_empty_entry_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        assert!(save(dir.path(), None, "  \n ", at(0)).is_err());
        assert!(entries(dir.path()).is_empty());
    }

    #[test]
    fn the_title_is_the_first_line_that_says_anything() {
        assert_eq!(title_of("\n\n  # Groceries \nmilk"), "Groceries");
        assert_eq!(title_of(""), "");
        let long = "word ".repeat(40);
        let title = title_of(&long);
        assert!(title.ends_with('…'), "got: {title}");
        assert!(title.chars().count() <= TITLE_CHARS + 1);
        // Multi-byte text is cut on characters, not bytes.
        assert_eq!(title_of(&"é".repeat(100)).chars().count(), TITLE_CHARS + 1);
    }

    #[test]
    fn search_finds_entries_by_their_words() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), None, "call the dentist about tuesday", at(1)).unwrap();
        save(dir.path(), None, "idea: a podcast about maps", at(2)).unwrap();
        let found = search("dentist", &entries(dir.path()));
        assert_eq!(found.len(), 1);
        assert!(found[0].note.content.contains("dentist"));
    }
}
