//! Everything the user says, kept as plain markdown and searchable months later.
//!
//! The files are the truth: `Memory/YYYY/MM/YYYY-MM-DD.md`, one per day, one
//! `## HH:MM` section per utterance. They open in any editor and survive the
//! app. Search runs against a SQLite FTS5 index beside them that holds nothing
//! the files don't — delete it and `open` rebuilds it, edit a file in another
//! editor and the next `sync` picks it up.
//!
//! ```text
//! # 2026-10-06
//!
//! ## 14:32
//! the pricing idea: charge per seat, not per minute
//!
//! ## 21:40 · journal
//! long day, but the demo landed
//! ```
//!
//! ponytail: keyword search with stemming and prefixes only. "cost of the plan"
//! will not find "pricing idea" — local embeddings stored in this same SQLite
//! file are the upgrade path.

use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";
const INDEX_FILE: &str = ".index.sqlite";
const JOURNAL_MARK: &str = " · journal";
const DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A thought said any time, anywhere.
    Thought,
    /// An answer to the end-of-day "how was your day?".
    Journal,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Thought => "thought",
            Kind::Journal => "journal",
        }
    }

    fn from_db(s: &str) -> Kind {
        if s == "journal" {
            Kind::Journal
        } else {
            Kind::Thought
        }
    }
}

/// One utterance. Date and time are strings (`2026-10-06`, `14:32`) so they
/// cross to TypeScript as-is and sort lexically.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Entry {
    pub date: String,
    pub time: String,
    pub kind: Kind,
    pub text: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct Hit {
    pub entry: Entry,
    /// Text around the match, for the results list.
    pub snippet: String,
}

#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Inclusive `YYYY-MM-DD` bounds.
    pub since: Option<String>,
    pub until: Option<String>,
    pub kind: Option<Kind>,
    /// Defaults to 20.
    pub limit: Option<usize>,
}

pub struct Memory {
    root: PathBuf,
    db: Connection,
}

/// `<app-data>/Aloud/Memory`.
pub fn directory() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("Memory"))
}

impl Memory {
    pub fn open_default() -> Result<Self> {
        Self::open(&directory()?)
    }

    /// Opens (creating if needed) the memory folder and brings the index up to
    /// date with whatever is on disk.
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
        let db = Connection::open(root.join(INDEX_FILE)).context("opening memory index")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                 path TEXT PRIMARY KEY, mtime INTEGER NOT NULL, size INTEGER NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS entries USING fts5(
                 text, path UNINDEXED, date UNINDEXED, time UNINDEXED, kind UNINDEXED,
                 tokenize = 'porter unicode61 remove_diacritics 2');",
        )
        .context("creating memory index")?;
        let mut memory = Memory {
            root: root.to_path_buf(),
            db,
        };
        memory.sync()?;
        Ok(memory)
    }

    /// Appends an utterance to the day's file and indexes it. Returns the file.
    /// `at` is the user's local time — the day boundary is theirs, not UTC's.
    pub fn add(&mut self, kind: Kind, text: &str, at: NaiveDateTime) -> Result<PathBuf> {
        let text = text.trim();
        if text.is_empty() {
            bail!("nothing to remember");
        }
        let path = self
            .root
            .join(at.format("%Y").to_string())
            .join(at.format("%m").to_string())
            .join(format!("{}.md", at.format(DATE_FORMAT)));
        fs::create_dir_all(path.parent().expect("day file has a parent"))
            .context("creating month folder")?;

        let is_new = !path.exists();
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        let mut block = String::new();
        if is_new {
            block.push_str(&format!("# {}\n", at.format(DATE_FORMAT)));
        }
        let mark = if kind == Kind::Journal {
            JOURNAL_MARK
        } else {
            ""
        };
        block.push_str(&format!(
            "\n## {}{mark}\n{}\n",
            at.format(TIME_FORMAT),
            escape(text)
        ));
        file.write_all(block.as_bytes())
            .with_context(|| format!("writing {}", path.display()))?;
        drop(file);

        self.index_file(&path)?;
        Ok(path)
    }

    /// Reconciles the index with the folder: new and edited files are re-read,
    /// vanished ones dropped. Cheap when nothing changed (a stat per file).
    pub fn sync(&mut self) -> Result<()> {
        let mut on_disk = Vec::new();
        collect_day_files(&self.root, &mut on_disk);

        let mut indexed: HashMap<String, (i64, i64)> = HashMap::new();
        {
            let mut stmt = self.db.prepare("SELECT path, mtime, size FROM files")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
            for row in rows {
                let (path, stamp) = row?;
                indexed.insert(path, stamp);
            }
        }

        let mut seen = HashSet::new();
        for path in &on_disk {
            let key = self.key(path);
            if indexed.get(&key) != stamp(path).as_ref() {
                self.index_file(path)?;
            }
            seen.insert(key);
        }
        for key in indexed.keys().filter(|k| !seen.contains(*k)) {
            self.forget(key)?;
        }
        Ok(())
    }

    /// Ranked full-text search: every word must appear (stemmed, so "pricing"
    /// finds "priced"), and the last word may be a prefix, so results appear
    /// while the query is still being typed.
    pub fn search(&self, query: &str, filter: &Filter) -> Result<Vec<Hit>> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.db.prepare(
            "SELECT text, path, date, time, kind, snippet(entries, 0, '', '', '…', 24)
             FROM entries
             WHERE entries MATCH ?1
               AND (?2 IS NULL OR date >= ?2)
               AND (?3 IS NULL OR date <= ?3)
               AND (?4 IS NULL OR kind = ?4)
             ORDER BY rank, date DESC, time DESC
             LIMIT ?5",
        )?;
        let kind = filter.kind.map(Kind::as_str);
        let limit = filter.limit.unwrap_or(DEFAULT_LIMIT);
        let rows = stmt.query_map(
            params![fts, filter.since, filter.until, kind, limit as i64],
            |r| {
                Ok(Hit {
                    entry: self.entry_from(r)?,
                    snippet: r.get::<_, String>(5)?.replace('\n', " "),
                })
            },
        )?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// Everything said on `date`, in order — what a journal summary reads.
    pub fn day(&self, date: NaiveDate) -> Result<Vec<Entry>> {
        let mut stmt = self.db.prepare(
            "SELECT text, path, date, time, kind FROM entries
             WHERE date = ?1 ORDER BY time",
        )?;
        let rows = stmt.query_map([date.format(DATE_FORMAT).to_string()], |r| {
            self.entry_from(r)
        })?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// Newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut stmt = self.db.prepare(
            "SELECT text, path, date, time, kind FROM entries
             ORDER BY date DESC, time DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| self.entry_from(r))?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// Whether the user already answered "how was your day?" on `date`, so the
    /// evening nudge can stay quiet.
    pub fn journaled_on(&self, date: NaiveDate) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                "SELECT 1 FROM entries WHERE date = ?1 AND kind = 'journal' LIMIT 1",
                [date.format(DATE_FORMAT).to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    fn entry_from(&self, r: &rusqlite::Row) -> rusqlite::Result<Entry> {
        Ok(Entry {
            text: r.get(0)?,
            path: self.root.join(r.get::<_, String>(1)?),
            date: r.get(2)?,
            time: r.get(3)?,
            kind: Kind::from_db(&r.get::<_, String>(4)?),
        })
    }

    /// Index key: the path relative to the root, so moving the folder does not
    /// invalidate the index.
    fn key(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    fn index_file(&mut self, path: &Path) -> Result<()> {
        let key = self.key(path);
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let date = date_of(path).context("day file name is not a date")?;
        let (mtime, size) = stamp(path).unwrap_or((0, 0));

        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM entries WHERE path = ?1", [&key])?;
        for e in parse(&content) {
            tx.execute(
                "INSERT INTO entries (text, path, date, time, kind) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![e.text, key, date, e.time, e.kind.as_str()],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO files (path, mtime, size) VALUES (?1, ?2, ?3)",
            params![key, mtime, size],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn forget(&mut self, key: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM entries WHERE path = ?1", [key])?;
        tx.execute("DELETE FROM files WHERE path = ?1", [key])?;
        tx.commit()?;
        Ok(())
    }
}

struct Parsed {
    time: String,
    kind: Kind,
    text: String,
}

type Section<'a> = (String, Kind, Vec<&'a str>);

/// Splits a day file into its `## HH:MM` sections. Anything before the first
/// section (the `# date` title) is ignored, and so is a section with no text.
fn parse(content: &str) -> Vec<Parsed> {
    fn flush(current: &mut Option<Section>, out: &mut Vec<Parsed>) {
        if let Some((time, kind, lines)) = current.take() {
            let text = lines.join("\n").trim().to_string();
            if !text.is_empty() {
                out.push(Parsed { time, kind, text });
            }
        }
    }

    let mut out = Vec::new();
    let mut current: Option<Section> = None;
    for line in content.lines() {
        if let Some((time, kind)) = header(line) {
            flush(&mut current, &mut out);
            current = Some((time, kind, Vec::new()));
        } else if let Some((_, _, lines)) = current.as_mut() {
            lines.push(line);
        }
    }
    flush(&mut current, &mut out);
    out
}

fn header(line: &str) -> Option<(String, Kind)> {
    let rest = line.strip_prefix("## ")?;
    let time = rest.get(..5)?;
    NaiveTime::parse_from_str(time, TIME_FORMAT).ok()?;
    let kind = if rest[5..].trim_start_matches([' ', '·']).trim() == "journal" {
        Kind::Journal
    } else {
        Kind::Thought
    };
    Some((time.to_string(), kind))
}

/// A dictated line that happens to look like a section header (`## 10:30 …`)
/// would split its entry in two; a backslash defuses it.
fn escape(text: &str) -> String {
    text.lines()
        .map(|l| {
            if header(l).is_some() {
                format!("\\{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Turns free text into an FTS5 query that cannot be a syntax error: each word
/// quoted, the last one a prefix. `None` when there is no word to search.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let (last, rest) = words.split_last()?;
    let mut parts: Vec<String> = rest.iter().map(|w| format!("\"{w}\"")).collect();
    parts.push(format!("\"{last}\"*"));
    Some(parts.join(" "))
}

fn date_of(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    NaiveDate::parse_from_str(stem, DATE_FORMAT).ok()?;
    Some(stem.to_string())
}

fn stamp(path: &Path) -> Option<(i64, i64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((mtime.as_nanos() as i64, meta.len() as i64))
}

/// Every `YYYY-MM-DD.md` under `dir`, skipping hidden entries (the index).
fn collect_day_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_day_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") && date_of(&path).is_some() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn open(dir: &tempfile::TempDir) -> Memory {
        Memory::open(&dir.path().join("Memory")).unwrap()
    }

    fn all(m: &Memory, q: &str) -> Vec<Hit> {
        m.search(q, &Filter::default()).unwrap()
    }

    #[test]
    fn writes_one_readable_file_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(Kind::Thought, "charge per seat", at("2026-10-06 14:32"))
            .unwrap();
        let path = m
            .add(Kind::Journal, "the demo landed", at("2026-10-06 21:40"))
            .unwrap();
        assert!(
            path.ends_with("2026/10/2026-10-06.md"),
            "got {}",
            path.display()
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "# 2026-10-06\n\n## 14:32\ncharge per seat\n\n## 21:40 · journal\nthe demo landed\n"
        );
    }

    #[test]
    fn finds_a_thought_months_later_by_a_different_word_form() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(
            Kind::Thought,
            "we priced it per seat",
            at("2026-04-02 09:00"),
        )
        .unwrap();
        m.add(Kind::Thought, "buy oat milk", at("2026-09-30 18:00"))
            .unwrap();
        let hits = all(&m, "pricing");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].entry.date, "2026-04-02");
    }

    #[test]
    fn last_word_is_a_prefix_so_search_works_while_typing() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(
            Kind::Thought,
            "the onboarding flow is too long",
            at("2026-10-06 10:00"),
        )
        .unwrap();
        assert_eq!(all(&m, "onboar").len(), 1);
        assert_eq!(all(&m, "flow onboar").len(), 1);
        assert!(all(&m, "flow billing").is_empty(), "every word must match");
    }

    /// FTS5 treats quotes, AND, `-` and `:` as syntax; free text must not.
    #[test]
    fn punctuation_in_a_query_is_not_a_syntax_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(Kind::Thought, "what's the plan", at("2026-10-06 10:00"))
            .unwrap();
        for q in [
            "what's \"the",
            "plan AND",
            "-plan",
            "col:plan",
            "((",
            "",
            "   ",
        ] {
            assert!(
                m.search(q, &Filter::default()).is_ok(),
                "query {q:?} errored"
            );
        }
        assert_eq!(all(&m, "what's the plan").len(), 1);
    }

    #[test]
    fn filters_by_kind_and_date_range() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(Kind::Thought, "launch idea", at("2026-01-10 10:00"))
            .unwrap();
        m.add(
            Kind::Journal,
            "launch day was tense",
            at("2026-02-10 22:00"),
        )
        .unwrap();
        m.add(Kind::Thought, "launch retro", at("2026-03-10 10:00"))
            .unwrap();

        let journal = Filter {
            kind: Some(Kind::Journal),
            ..Filter::default()
        };
        assert_eq!(m.search("launch", &journal).unwrap().len(), 1);

        let feb_on = Filter {
            since: Some("2026-02-01".into()),
            ..Filter::default()
        };
        assert_eq!(m.search("launch", &feb_on).unwrap().len(), 2);

        let only_feb = Filter {
            since: Some("2026-02-01".into()),
            until: Some("2026-02-28".into()),
            ..Filter::default()
        };
        assert_eq!(m.search("launch", &only_feb).unwrap().len(), 1);
    }

    #[test]
    fn edits_made_outside_the_app_are_picked_up_by_sync() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        let path = m
            .add(Kind::Thought, "old words", at("2026-10-06 10:00"))
            .unwrap();
        fs::write(&path, "# 2026-10-06\n\n## 10:00\nrewritten in obsidian\n").unwrap();
        assert!(all(&m, "rewritten").is_empty(), "not indexed until sync");
        m.sync().unwrap();
        assert_eq!(all(&m, "rewritten").len(), 1);
        assert!(all(&m, "old").is_empty());
    }

    #[test]
    fn deleted_files_leave_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        let path = m
            .add(Kind::Thought, "forget me", at("2026-10-06 10:00"))
            .unwrap();
        fs::remove_file(path).unwrap();
        m.sync().unwrap();
        assert!(all(&m, "forget").is_empty());
    }

    /// The index is disposable: a lost `.index.sqlite` must cost nothing but a
    /// rebuild.
    #[test]
    fn index_rebuilds_from_the_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Memory");
        {
            let mut m = Memory::open(&root).unwrap();
            m.add(Kind::Journal, "a quiet sunday", at("2026-10-04 20:00"))
                .unwrap();
        }
        fs::remove_file(root.join(INDEX_FILE)).unwrap();
        let m = Memory::open(&root).unwrap();
        assert_eq!(all(&m, "sunday").len(), 1);
        assert!(m
            .journaled_on(NaiveDate::from_ymd_opt(2026, 10, 4).unwrap())
            .unwrap());
    }

    #[test]
    fn journaled_on_only_counts_journal_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        let day = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
        m.add(Kind::Thought, "just a thought", at("2026-10-06 10:00"))
            .unwrap();
        assert!(!m.journaled_on(day).unwrap(), "a thought is not a journal");
        m.add(Kind::Journal, "how the day went", at("2026-10-06 21:00"))
            .unwrap();
        assert!(m.journaled_on(day).unwrap());
    }

    #[test]
    fn day_returns_entries_in_clock_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(Kind::Thought, "evening", at("2026-10-06 19:00"))
            .unwrap();
        m.add(Kind::Thought, "morning", at("2026-10-06 08:00"))
            .unwrap();
        let day = m
            .day(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap())
            .unwrap();
        let times: Vec<_> = day.iter().map(|e| e.time.as_str()).collect();
        assert_eq!(times, ["08:00", "19:00"]);
    }

    #[test]
    fn recent_is_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(Kind::Thought, "first", at("2026-10-05 10:00"))
            .unwrap();
        m.add(Kind::Thought, "second", at("2026-10-06 10:00"))
            .unwrap();
        assert_eq!(m.recent(5).unwrap()[0].text, "second");
    }

    /// A dictated line shaped like a section header must not split its entry.
    #[test]
    fn dictated_text_that_looks_like_a_header_stays_one_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(
            Kind::Thought,
            "agenda\n## 10:30 standup\nthen lunch",
            at("2026-10-06 09:00"),
        )
        .unwrap();
        let day = m
            .day(NaiveDate::from_ymd_opt(2026, 10, 6).unwrap())
            .unwrap();
        assert_eq!(day.len(), 1);
        assert!(day[0].text.contains("then lunch"));
    }

    #[test]
    fn rejects_empty_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        assert!(m
            .add(Kind::Thought, "  \n ", at("2026-10-06 09:00"))
            .is_err());
    }

    #[test]
    fn searches_non_ascii_text() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = open(&dir);
        m.add(
            Kind::Thought,
            "Møte med Åse på café",
            at("2026-10-06 09:00"),
        )
        .unwrap();
        assert_eq!(all(&m, "cafe").len(), 1, "accents are folded");
        assert_eq!(all(&m, "åse").len(), 1);
        assert_eq!(
            all(&m, "møte").len(),
            1,
            "letters like ø still match as typed"
        );
    }

    #[test]
    fn other_markdown_files_in_the_folder_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Memory");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("README.md"), "## 10:00\nnot a day file").unwrap();
        let m = Memory::open(&root).unwrap();
        assert!(all(&m, "day").is_empty());
    }
}
