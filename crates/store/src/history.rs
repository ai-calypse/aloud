//! Dictation history, ported from `PipelineHistoryStore.swift`.
//!
//! rusqlite (bundled SQLite) replaces Core Data. The Swift version built its
//! model programmatically, so there is no schema to migrate — just a table.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

/// Specta forbids exporting i64 by default, since JS loses precision past
/// 2^53. These are row ids, unix seconds and millisecond durations — all
/// many orders of magnitude below that — so `number` is exact here and
/// BigInt would only make the frontend painful.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[specta(type = specta_typescript::Number)]
    pub id: i64,
    /// Unix seconds.
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    /// What the transcriber returned.
    pub raw_transcript: String,
    /// What was actually pasted, after cleanup.
    pub final_transcript: String,
    /// Which app had focus, for per-app rules and debugging.
    pub app_name: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: i64,
}

pub struct History {
    conn: Connection,
}

impl History {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at       INTEGER NOT NULL,
                raw_transcript   TEXT NOT NULL,
                final_transcript TEXT NOT NULL,
                app_name         TEXT,
                duration_ms      INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS history_created_at ON history (created_at DESC);",
        )
        .context("creating history schema")?;
        Ok(())
    }

    /// Records one dictation and prunes to `retention` newest entries.
    pub fn insert(
        &self,
        created_at: i64,
        raw: &str,
        final_text: &str,
        app_name: Option<&str>,
        duration_ms: i64,
        retention: usize,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO history (created_at, raw_transcript, final_transcript, app_name, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![created_at, raw, final_text, app_name, duration_ms],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune(retention)?;
        Ok(id)
    }

    /// Newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, raw_transcript, final_transcript, app_name, duration_ms
             FROM history ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(Entry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                raw_transcript: row.get(2)?,
                final_transcript: row.get(3)?,
                app_name: row.get(4)?,
                duration_ms: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("reading history")
    }

    /// Keeps the newest `retention` entries.
    fn prune(&self, retention: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM history WHERE id NOT IN (
                SELECT id FROM history ORDER BY created_at DESC, id DESC LIMIT ?1
             )",
            [retention as i64],
        )?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(history: &History, at: i64, text: &str, retention: usize) {
        history
            .insert(at, text, text, Some("TestApp"), 100, retention)
            .unwrap();
    }

    #[test]
    fn stores_and_returns_newest_first() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "first", 50);
        insert(&history, 200, "second", 50);

        let recent = history.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].final_transcript, "second");
        assert_eq!(recent[0].app_name.as_deref(), Some("TestApp"));
    }

    #[test]
    fn prunes_to_retention_limit() {
        let history = History::open_in_memory().unwrap();
        for i in 0..10 {
            insert(&history, i, &format!("entry {i}"), 3);
        }

        let all = history.recent(100).unwrap();
        assert_eq!(all.len(), 3, "only the newest 3 survive");
        assert_eq!(all[0].final_transcript, "entry 9");
    }

    /// Entries written within the same second must still order deterministically,
    /// which is why id breaks the tie.
    #[test]
    fn same_timestamp_orders_by_insertion() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "older", 50);
        insert(&history, 100, "newer", 50);

        let recent = history.recent(10).unwrap();
        assert_eq!(recent[0].final_transcript, "newer");
    }

    #[test]
    fn survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");

        let history = History::open(&path).unwrap();
        insert(&history, 100, "persisted", 50);
        drop(history);

        let reopened = History::open(&path).unwrap();
        assert_eq!(
            reopened.recent(10).unwrap()[0].final_transcript,
            "persisted"
        );
    }

    #[test]
    fn clear_empties_the_table() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "gone", 50);
        history.clear().unwrap();
        assert!(history.recent(10).unwrap().is_empty());
    }
}
