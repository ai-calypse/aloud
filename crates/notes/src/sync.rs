//! Which of Aloud's notes the Apple Notes app already has a copy of.
//!
//! The copy is one-way, Aloud to Notes. This module is only the bookkeeping:
//! per note, the id Notes gave its copy and a fingerprint of what was sent, so
//! the next sync updates that same note instead of adding a second one, and
//! can tell a note edited since from one that is current. Talking to Notes is
//! the caller's `push`.
//!
//! ponytail: one-way. Two-way sync (an edit made in Notes coming back) needs
//! Notes' modification date compared against this fingerprint plus a merge
//! rule for edits on both sides; start from `Entry` if that is ever wanted.

use crate::Note;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// `<app-data>/Aloud/notes-sync.json`. Beside the notes folder, not in it, so
/// it is never listed as a note.
pub fn ledger_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(crate::data_dir()?.join("notes-sync.json"))
}

/// Where a note stands against its copy in Notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncState {
    /// Never copied.
    New,
    /// The copy in Notes matches.
    Synced,
    /// Copied once, then changed here.
    Edited,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct SyncMark {
    pub path: String,
    pub state: SyncState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    /// The Notes id (`x-coredata://…`) of the copy.
    id: String,
    /// Fingerprint of the markdown that copy was made from.
    hash: String,
}

/// The saved id and fingerprint per note, keyed by file name: the notes folder
/// is fixed, and a name survives the folder being moved, an absolute path
/// would not.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Ledger(BTreeMap<String, Entry>);

/// FNV-1a, because it is written to disk: `DefaultHasher` may change between
/// Rust releases, which would mark every note edited after an update.
fn fingerprint(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn key(note: &Note) -> String {
    note.path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

impl Ledger {
    /// A missing or unreadable file is an empty ledger: the worst case is a
    /// note copied twice, never a lost one.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn state(&self, note: &Note) -> SyncState {
        match self.0.get(&key(note)) {
            None => SyncState::New,
            Some(e) if e.hash == fingerprint(&note.content) => SyncState::Synced,
            Some(_) => SyncState::Edited,
        }
    }

    /// The Notes id of this note's copy, if it has one.
    pub fn apple_id(&self, note: &Note) -> Option<&str> {
        self.0.get(&key(note)).map(|e| e.id.as_str())
    }

    fn record(&mut self, note: &Note, id: String) {
        self.0.insert(
            key(note),
            Entry {
                id,
                hash: fingerprint(&note.content),
            },
        );
    }
}

/// Copies every note in `notes` that is new or edited, handing `push` the note
/// and its existing Notes id (None for a first copy) and recording the id it
/// returns. The ledger is written after each success, so a failure part-way
/// keeps what already went through. Returns how many were copied and the error
/// that stopped the run, if any.
///
/// ponytail: stops at the first error rather than skipping that note — a
/// denied Automation permission would otherwise fail (and wait out osascript's
/// timeout) once per note. Per-note retry is the upgrade if one stubborn note
/// ever blocks the rest.
pub fn sync_pending<E>(
    notes: &[Note],
    ledger_path: &Path,
    mut push: impl FnMut(&Note, Option<&str>) -> Result<String, E>,
) -> (usize, Option<E>) {
    let mut ledger = Ledger::load(ledger_path);
    let mut pushed = 0;
    for note in notes {
        if ledger.state(note) == SyncState::Synced {
            continue;
        }
        match push(note, ledger.apple_id(note)) {
            Ok(id) => {
                ledger.record(note, id);
                // The copy exists in Notes whether or not this write lands;
                // losing the id only costs a duplicate next time.
                let _ = ledger.save(ledger_path);
                pushed += 1;
            }
            Err(e) => return (pushed, Some(e)),
        }
    }
    (pushed, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn note(name: &str, content: &str) -> Note {
        Note {
            path: PathBuf::from(format!("/notes/{name}.md")),
            title: name.into(),
            date: 0,
            content: content.into(),
        }
    }

    #[test]
    fn a_note_is_new_then_synced_then_edited() {
        let mut ledger = Ledger::default();
        let n = note("a", "milk");
        assert_eq!(ledger.state(&n), SyncState::New);
        ledger.record(&n, "id-1".into());
        assert_eq!(ledger.state(&n), SyncState::Synced);
        assert_eq!(ledger.state(&note("a", "milk, eggs")), SyncState::Edited);
        assert_eq!(ledger.apple_id(&n), Some("id-1"));
    }

    /// The failure this exists for: syncing twice added a second copy in
    /// Notes. The second run must hand `push` the first run's id.
    #[test]
    fn a_second_sync_updates_the_same_notes_note_instead_of_adding_one() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("ledger.json");
        let mut seen: Vec<Option<String>> = Vec::new();
        let mut push = |_: &Note, id: Option<&str>| -> Result<String, ()> {
            seen.push(id.map(str::to_owned));
            Ok("id-1".to_string())
        };

        let (n, err) = sync_pending(&[note("a", "v1")], &ledger_path, &mut push);
        assert_eq!((n, err), (1, None));
        let (n, _) = sync_pending(&[note("a", "v2")], &ledger_path, &mut push);
        assert_eq!(n, 1);
        // Unchanged since: nothing to push.
        let (n, _) = sync_pending(&[note("a", "v2")], &ledger_path, &mut push);
        assert_eq!(n, 0);
        assert_eq!(seen, vec![None, Some("id-1".to_string())]);
    }

    #[test]
    fn a_failure_keeps_the_notes_already_copied() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("ledger.json");
        let notes = [note("a", "1"), note("b", "2"), note("c", "3")];
        let (n, err) = sync_pending(&notes, &ledger_path, |n, _| {
            if n.title == "b" {
                Err("denied")
            } else {
                Ok(format!("id-{}", n.title))
            }
        });
        assert_eq!((n, err), (1, Some("denied")));
        let ledger = Ledger::load(&ledger_path);
        assert_eq!(ledger.state(&notes[0]), SyncState::Synced);
        assert_eq!(ledger.state(&notes[1]), SyncState::New);
        assert_eq!(ledger.state(&notes[2]), SyncState::New);
    }

    #[test]
    fn a_corrupt_ledger_reads_as_empty_not_as_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Ledger::load(&path).state(&note("a", "x")), SyncState::New);
    }

    /// The fingerprint is stored; changing it would mark every note edited.
    #[test]
    fn the_fingerprint_is_stable_across_releases() {
        assert_eq!(fingerprint("hello"), "a430d84680aabd0b");
    }
}
