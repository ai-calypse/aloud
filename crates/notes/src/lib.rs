//! App-managed home for notes: plain markdown files under the OS app-data
//! directory, keyword-searched by loading every note into memory. Every query
//! term must appear in the note; title hits outrank body hits.
//!
//! ponytail: linear scan of all note files per query — plenty for years of
//! meetings; SQLite FTS is the upgrade path if it ever feels slow.

pub mod sync;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H%M";
const SNIPPET_LEN: usize = 160;
const SNIPPET_LEAD: usize = 80;
const TITLE_WEIGHT: usize = 10;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    /// Unix seconds; parsed from the file name, falling back to file mtime.
    /// Exported as `number` — specta forbids i64 by default over JS precision
    /// loss past 2^53, which a unix second is nowhere near.
    #[specta(type = specta_typescript::Number)]
    pub date: i64,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SearchResult {
    pub note: Note,
    #[specta(type = specta_typescript::Number)]
    pub score: usize,
    /// Text around the first body match, for the results list.
    pub snippet: String,
}

/// The app's data directory: `~/Library/Application Support/Aloud` on macOS,
/// `%APPDATA%\Aloud` on Windows.
///
/// Never `ProjectDirs::data_dir()`: on macOS it yields `app.Aloud.Aloud`, while
/// existing installs (and migration) read a plain `Aloud` folder.
pub fn data_dir() -> Result<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        directories::UserDirs::new()
            .context("no home directory available")?
            .home_dir()
            .join("Library/Application Support")
    } else {
        directories::BaseDirs::new()
            .context("no home directory available")?
            .data_dir()
            .to_path_buf()
    };
    Ok(base.join("Aloud"))
}

/// `<app-data>/Aloud/Notes`, created on demand.
pub fn directory() -> Result<PathBuf> {
    Ok(data_dir()?.join("Notes"))
}

/// `<app-data>/Aloud/media`, where kept recordings live. Its own folder because
/// the asset-protocol scope in `tauri.conf.json` points the WebView at exactly
/// this path; widening it would expose settings, history and every note.
pub fn media_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("media"))
}

/// A unique path for one recording, with the media directory created.
/// Nanoseconds, not a counter: two windows or a restart mid-second must not be
/// able to hand out the same name and overwrite another dictation's audio.
pub fn media_wav_path() -> Result<PathBuf> {
    let dir = media_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(dir.join(format!("aloud-{unique}.wav")))
}

/// All notes on disk, newest first.
pub fn all_notes(dir: &Path) -> Vec<Note> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut notes: Vec<Note> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
        .filter_map(|p| note_from(&p))
        .collect();
    notes.sort_by_key(|n| std::cmp::Reverse(n.date));
    notes
}

pub fn note_from(path: &Path) -> Option<Note> {
    let content = fs::read_to_string(path).ok()?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let date = date_from_file_name(&stem)
        .or_else(|| file_mtime(path))
        .unwrap_or(0);
    Some(Note {
        title: title_from_markdown(&content).unwrap_or_else(|| stem.clone()),
        path: path.to_path_buf(),
        date,
        content,
    })
}

fn file_mtime(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).timestamp())
}

/// First `# ` heading in the markdown, if any.
pub fn title_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .filter_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .find(|title| !title.is_empty())
        .map(str::to_owned)
}

/// Parses the "yyyy-MM-dd HHmm" prefix that note file names carry.
pub fn date_from_file_name(name: &str) -> Option<i64> {
    let prefix: String = name.chars().take(15).collect();
    NaiveDateTime::parse_from_str(&prefix, FILE_TIMESTAMP_FORMAT)
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}

/// Query terms, shared with the UI so highlighting matches what scored.
pub fn terms(query: &str) -> Vec<&str> {
    query.split_whitespace().collect()
}

/// Case-insensitive keyword match: every term must appear somewhere in the
/// note; title hits outrank body hits.
pub fn search(query: &str, notes: &[Note]) -> Vec<SearchResult> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut results: Vec<SearchResult> = notes
        .iter()
        .filter_map(|note| {
            let mut score = 0;
            let mut first_body_match: Option<usize> = None;

            for term in &terms {
                let title_hits = match_indices(&note.title, term).count();
                let body: Vec<usize> = match_indices(&note.content, term).collect();
                if title_hits == 0 && body.is_empty() {
                    return None;
                }
                score += title_hits * TITLE_WEIGHT + body.len();
                if let Some(&first) = body.first() {
                    first_body_match = Some(first_body_match.map_or(first, |cur| cur.min(first)));
                }
            }

            Some(SearchResult {
                snippet: snippet(&note.content, first_body_match),
                note: note.clone(),
                score,
            })
        })
        .collect();

    results.sort_by_key(|r| std::cmp::Reverse(r.score));
    results
}

/// Byte offsets of every case-insensitive occurrence of `term` in `text`.
fn match_indices<'a>(text: &'a str, term: &'a str) -> impl Iterator<Item = usize> + 'a {
    let haystack = text.to_lowercase();
    let needle = term.to_lowercase();
    let mut start = 0;
    std::iter::from_fn(move || {
        if needle.is_empty() || start >= haystack.len() {
            return None;
        }
        let found = haystack[start..].find(&needle)? + start;
        start = found + needle.len();
        Some(found)
    })
}

/// ~160 characters of context around the match (the note's opening when the
/// match was title-only), flattened to one line.
pub fn snippet(content: &str, around: Option<usize>) -> String {
    let chars: Vec<char> = content.chars().collect();
    // `around` is a byte offset; convert to a char index for slicing.
    let match_char = around.map(|byte| content[..byte].chars().count());
    let start = match_char.map_or(0, |i| i.saturating_sub(SNIPPET_LEAD));
    let end = (start + SNIPPET_LEN).min(chars.len());

    let mut text: String = chars[start..end]
        .iter()
        .collect::<String>()
        .replace('\n', " ")
        .replace('#', "")
        .trim()
        .to_string();

    if start > 0 {
        text.insert(0, '…');
    }
    if end < chars.len() {
        text.push('…');
    }
    text
}

/// Writes `content` as a new note named `<yyyy-MM-dd HHmm> <title>.md`,
/// suffixing a counter if that name is taken. Returns the path written.
pub fn write_note(dir: &Path, title: &str, content: &str, at: DateTime<Utc>) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("creating notes dir {}", dir.display()))?;
    let stem = format!("{} {}", at.format(FILE_TIMESTAMP_FORMAT), sanitize(title));

    let mut path = dir.join(format!("{stem}.md"));
    let mut counter = 2;
    while path.exists() {
        path = dir.join(format!("{stem} {counter}.md"));
        counter += 1;
    }

    fs::write(&path, content).with_context(|| format!("writing note {}", path.display()))?;
    Ok(path)
}

/// What a note typed into the notch is called on disk. It is also how the
/// notch finds them again: meeting notes and voice notes live in the same
/// folder and are not the notch's to edit.
pub const QUICK_TITLE: &str = "Quick note";

/// The notch's quick notes, newest first.
pub fn quick_notes(dir: &Path) -> Vec<Note> {
    let mut notes = all_notes(dir);
    notes.retain(|n| is_quick(&n.path));
    notes
}

fn is_quick(path: &Path) -> bool {
    path.file_stem()
        .is_some_and(|s| s.to_string_lossy().contains(QUICK_TITLE))
}

/// Saves a quick note: a new file when `path` is None, else that note
/// overwritten. Called on every pause in typing, so it must be safe to call
/// with whatever the webview sends — `path` has to be a quick note sitting
/// directly in `dir`, or nothing is written.
pub fn save_quick(
    dir: &Path,
    path: Option<&Path>,
    text: &str,
    at: DateTime<Utc>,
) -> Result<PathBuf> {
    let Some(path) = path else {
        return write_note(dir, QUICK_TITLE, text, at);
    };
    anyhow::ensure!(
        path.parent() == Some(dir) && is_quick(path),
        "{} is not a quick note",
        path.display()
    );
    fs::write(path, text).with_context(|| format!("writing note {}", path.display()))?;
    Ok(path.to_path_buf())
}

/// Strips path separators and characters Windows rejects in file names.
fn sanitize(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: &str, content: &str) -> Note {
        Note {
            path: PathBuf::from(format!("/tmp/{title}.md")),
            title: title.into(),
            date: 0,
            content: content.into(),
        }
    }

    #[test]
    fn all_terms_must_match() {
        let notes = [
            note("Standup", "Discussed the roadmap and hiring plans."),
            note("1:1 with Sam", "Discussed hiring only."),
        ];
        let results = search("hiring roadmap", &notes);
        assert_eq!(results.len(), 1, "only the note with every term matches");
        assert_eq!(results[0].note.title, "Standup");
        assert_eq!(search("HIRING", &notes).len(), 2, "case-insensitive");
    }

    #[test]
    fn title_hits_outrank_body_hits() {
        let notes = [
            note("Weekly sync", "We talked about the budget, budget, budget."),
            note("Budget review", "Quarterly numbers."),
        ];
        let results = search("budget", &notes);
        assert_eq!(results[0].note.title, "Budget review");
    }

    #[test]
    fn snippet_surrounds_match() {
        let padding = "x".repeat(200);
        let content = format!("{padding} the launch date moved to March {padding}");
        let results = search("launch", &[note("Note", &content)]);
        let snippet = &results[0].snippet;
        assert!(snippet.contains("launch date moved"), "got: {snippet}");
        assert!(
            snippet.starts_with('…') && snippet.ends_with('…'),
            "got: {snippet}"
        );
    }

    #[test]
    fn snippet_without_match_uses_opening() {
        let snippet = snippet("# Heading\nFirst line of the note.", None);
        assert!(snippet.starts_with("Heading"), "got: {snippet}");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let notes = [note("A", "body")];
        assert!(search("", &notes).is_empty());
        assert!(search("   ", &notes).is_empty());
    }

    #[test]
    fn file_name_date_parsing() {
        let date = date_from_file_name("2026-07-14 0930 Standup Notes").expect("should parse");
        let parsed = DateTime::from_timestamp(date, 0).unwrap();
        assert_eq!(
            parsed.format("%Y-%m-%d %H:%M").to_string(),
            "2026-07-14 09:30"
        );
        assert!(date_from_file_name("Untitled").is_none());
    }

    /// Multi-byte characters must not panic the snippet slicer.
    #[test]
    fn searches_non_ascii_notes() {
        let notes = [note(
            "M\u{00f8}te\u{00e5}pning",
            "M\u{00f8}tet startet med en kort \u{00e5}pning hvor teamet diskuterte fremdrift.",
        )];
        let results = search("m\u{00f8}te", &notes);
        assert_eq!(results.len(), 1);
        assert!(
            results[0].snippet.contains("M\u{00f8}tet"),
            "got: {}",
            results[0].snippet
        );
    }

    #[test]
    fn title_parsing() {
        assert_eq!(
            title_from_markdown("intro\n# Real Title\nbody").as_deref(),
            Some("Real Title")
        );
        assert!(title_from_markdown("no heading here").is_none());
    }

    #[test]
    fn write_note_avoids_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let at = DateTime::from_timestamp(0, 0).unwrap();
        let first = write_note(dir.path(), "Standup", "# Standup", at).unwrap();
        let second = write_note(dir.path(), "Standup", "# Standup", at).unwrap();
        assert_ne!(first, second, "second write must not clobber the first");
        assert_eq!(all_notes(dir.path()).len(), 2);
    }

    #[test]
    fn write_note_sanitizes_path_separators() {
        let dir = tempfile::tempdir().unwrap();
        let at = DateTime::from_timestamp(0, 0).unwrap();
        let path = write_note(dir.path(), "1/2 sync: recap", "body", at).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(!name.contains('/') && !name.contains(':'), "got: {name}");
    }

    /// The webview names the file to overwrite; it must not be able to name
    /// anything but a quick note in the notes folder (a meeting note, or a
    /// path that climbs out of it).
    #[test]
    fn save_quick_only_overwrites_quick_notes_in_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let at = DateTime::from_timestamp(0, 0).unwrap();
        let meeting = write_note(dir.path(), "Standup", "keep me", at).unwrap();
        assert!(save_quick(dir.path(), Some(&meeting), "x", at).is_err());
        assert_eq!(fs::read_to_string(&meeting).unwrap(), "keep me");

        let outside = dir.path().join("../escape Quick note.md");
        assert!(save_quick(dir.path(), Some(&outside), "x", at).is_err());

        let first = save_quick(dir.path(), None, "one", at).unwrap();
        let again = save_quick(dir.path(), Some(&first), "one two", at).unwrap();
        assert_eq!(first, again);
        assert_eq!(fs::read_to_string(&first).unwrap(), "one two");
        assert_eq!(
            quick_notes(dir.path()).len(),
            1,
            "the meeting note is not listed"
        );
    }
}
