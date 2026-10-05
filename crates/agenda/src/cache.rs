//! The last reminders read, in memory and on disk, so the view paints the
//! moment it opens and the read that follows only corrects it. Reminders are
//! read through another app, and "a local app should be instant" is the
//! user's bar, not ours.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::Reminder;

pub struct Store {
    path: PathBuf,
    mem: Mutex<Option<Vec<Reminder>>>,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            mem: Mutex::new(None),
        }
    }

    fn mem(&self) -> std::sync::MutexGuard<'_, Option<Vec<Reminder>>> {
        self.mem.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Memory first, else the file from the last run. `None` only when there
    /// has never been a read (or the file is unreadable, which is the same
    /// to the view: it waits for the real one).
    pub fn get(&self) -> Option<Vec<Reminder>> {
        let mut mem = self.mem();
        if mem.is_none() {
            *mem = load(&self.path);
        }
        mem.clone()
    }

    /// Remembers a fresh read, and writes it down only when it changed.
    pub fn put(&self, items: Vec<Reminder>) {
        let mut mem = self.mem();
        if mem.as_ref() != Some(&items) {
            save(&self.path, &items);
            *mem = Some(items);
        }
    }

    /// Drops one reminder, for the tick that just completed it.
    pub fn remove(&self, id: &str) {
        if let Some(mut items) = self.get() {
            items.retain(|r| r.id != id);
            self.put(items);
        }
    }
}

fn load(path: &Path) -> Option<Vec<Reminder>> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn save(path: &Path, items: &[Reminder]) {
    // Best effort: a cache that cannot be written is just a cold next launch.
    let Ok(json) = serde_json::to_vec(items) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// The app's one reminders cache, beside the other app data.
pub fn reminders() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| {
        let dir = aloud_notes::data_dir().unwrap_or_else(|_| std::env::temp_dir());
        let _ = std::fs::create_dir_all(&dir);
        Store::new(dir.join("reminders-cache.json"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reminder(id: &str) -> Reminder {
        Reminder {
            id: id.into(),
            title: id.into(),
            due: 1,
            all_day: false,
            list: "Reminders".into(),
        }
    }

    /// The first open after a launch is the one the user waits through: what
    /// the last run saw must be there before any read.
    #[test]
    fn a_new_process_starts_from_what_the_last_one_saw() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        Store::new(path.clone()).put(vec![reminder("a"), reminder("b")]);

        let next_launch = Store::new(path);
        let ids: Vec<_> = next_launch
            .get()
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn no_file_or_a_damaged_one_is_no_cache_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        assert!(Store::new(path.clone()).get().is_none());
        std::fs::write(&path, "{ not json").unwrap();
        assert!(Store::new(path).get().is_none());
    }

    #[test]
    fn a_completed_reminder_leaves_the_cache_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        let store = Store::new(path.clone());
        store.put(vec![reminder("a"), reminder("b")]);
        store.remove("a");
        assert_eq!(Store::new(path).get().unwrap().len(), 1);
    }

    #[test]
    fn an_unchanged_read_does_not_touch_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.json");
        let store = Store::new(path.clone());
        store.put(vec![reminder("a")]);
        let first = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::remove_file(&path).unwrap();
        store.put(vec![reminder("a")]);
        assert!(
            !path.exists(),
            "same items, so no write (first write was {first:?})"
        );
    }
}
