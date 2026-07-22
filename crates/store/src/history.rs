//! Dictation history, in a bundled-SQLite table.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

/// The i64s are exported as `number`: specta forbids that by default (JS loses
/// precision past 2^53), but row ids, unix seconds and ms durations are all far
/// below it, and BigInt would only make the frontend painful.
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
    pub app_name: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: i64,
    /// Joins this row to its `timings` spans. Null on rows written before the
    /// column existed, so its absence is normal, not missing data.
    #[specta(type = Option<specta_typescript::Number>)]
    pub utterance_id: Option<i64>,
    /// The recording this run came from, while it survives the audio budget.
    /// The transcript outlives the WAV, so this is often null.
    pub audio_path: Option<String>,
}

/// What the history is allowed to keep. Entries are capped by count,
/// recordings by size and age: a WAV is four orders of magnitude larger than
/// its row, so one number cannot bound both.
#[derive(Debug, Clone, Copy)]
pub struct Retention {
    pub entries: usize,
    /// Total recording bytes to keep. 0 keeps no audio at all.
    pub audio_bytes: u64,
    /// Evict recordings older than this many days. 0 means no age limit.
    pub audio_days: i64,
}

impl Retention {
    /// For callers that only care about the row count.
    pub fn entries(entries: usize) -> Self {
        Self {
            entries,
            audio_bytes: 0,
            audio_days: 0,
        }
    }
}

/// Removes a recording, tolerating one already gone: a file deleted by hand
/// must not wedge the prune that keeps the disk bounded.
fn remove_recording(path: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(%e, path, "could not remove a recording"),
    }
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

        // The batch above is the *original* table and must stay that way:
        // `CREATE TABLE IF NOT EXISTS` is a no-op on an existing database, so a
        // column added there reaches fresh installs only and every upgraded
        // machine fails at the first SELECT. New columns go in the ALTERs below,
        // gated on `user_version` so a real failure still surfaces.
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "ALTER TABLE history ADD COLUMN utterance_id INTEGER;
                 PRAGMA user_version = 1;",
            )
            .context("adding history.utterance_id")?;
        }
        if version < 2 {
            conn.execute_batch(
                "ALTER TABLE history ADD COLUMN audio_path TEXT;
                 PRAGMA user_version = 2;",
            )
            .context("adding history.audio_path")?;
        }
        Ok(())
    }

    /// Records one dictation and prunes to `retention` newest entries.
    // One argument per column, and the columns are what they are. A params
    // struct would be an `Entry` without its id, which the sole caller would
    // then have to build just to take apart again.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &self,
        created_at: i64,
        raw: &str,
        final_text: &str,
        app_name: Option<&str>,
        duration_ms: i64,
        utterance_id: Option<i64>,
        audio_path: Option<&str>,
        retention: Retention,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO history (created_at, raw_transcript, final_transcript, app_name, duration_ms, utterance_id, audio_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![created_at, raw, final_text, app_name, duration_ms, utterance_id, audio_path],
        )?;
        let id = self.conn.last_insert_rowid();
        self.prune(retention)?;
        Ok(id)
    }

    /// Newest first.
    pub fn recent(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT} ORDER BY created_at DESC, id DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map([limit as i64], read_entry)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("reading history")
    }

    /// One entry, or None if it has since been pruned or deleted.
    pub fn get(&self, id: i64) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(&format!("{SELECT} WHERE id = ?1"))?;
        stmt.query_row([id], read_entry)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .context("reading a history entry")
    }

    /// Applies the retention budget. Size and age take only the *recording*;
    /// the entry count takes the whole row, recording included.
    fn prune(&self, retention: Retention) -> Result<()> {
        let cutoff = if retention.audio_days > 0 {
            chrono::Utc::now().timestamp() - retention.audio_days * 86_400
        } else {
            i64::MIN
        };

        // Newest first: spend the budget on what is likeliest to be replayed.
        let mut doomed: Vec<(i64, String)> = Vec::new();
        let mut kept: u64 = 0;
        let mut over = false;
        {
            let mut stmt = self.conn.prepare(
                "SELECT id, created_at, audio_path FROM history
                 WHERE audio_path IS NOT NULL ORDER BY created_at DESC, id DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (id, created_at, path) = row?;
                // ponytail: one stat per kept recording per dictation. The
                // entry cap keeps that in the hundreds, so it is microseconds;
                // a `bytes` column written at insert is the upgrade path if
                // the cap ever grows.
                //
                // A vanished recording counts as zero rather than aborting: the
                // row still needs its now-dead path cleared.
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if !over && created_at >= cutoff {
                    kept += size;
                    if kept <= retention.audio_bytes {
                        continue;
                    }
                    // This one tipped the budget, so it and everything older
                    // go — keeping a smaller older one would leave holes.
                    over = true;
                }
                doomed.push((id, path));
            }
        }
        for (id, path) in doomed {
            self.release_audio(id, &path)?;
        }

        // Unlink before the DELETE, never after: a crash between the two leaves
        // an orphan file a directory sweep can find, where the reverse leaves a
        // row pointing at nothing and no record of where the audio was.
        self.unlink_and_delete(
            "SELECT audio_path FROM history WHERE audio_path IS NOT NULL AND id NOT IN (
                SELECT id FROM history ORDER BY created_at DESC, id DESC LIMIT ?1)",
            "DELETE FROM history WHERE id NOT IN (
                SELECT id FROM history ORDER BY created_at DESC, id DESC LIMIT ?1)",
            rusqlite::params![retention.entries as i64],
        )
    }

    /// Unlinks a recording and only then forgets where it was — see the
    /// ordering note in [`History::prune`].
    fn release_audio(&self, id: i64, path: &str) -> Result<()> {
        remove_recording(path);
        self.conn
            .execute("UPDATE history SET audio_path = NULL WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Removes every recording `select` names, then runs `delete`. One helper so
    /// no row-deleting path can forget the order.
    fn unlink_and_delete(
        &self,
        select: &str,
        delete: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<()> {
        let paths: Vec<String> = {
            let mut stmt = self.conn.prepare(select)?;
            let paths = stmt
                .query_map(params, |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            paths
        };
        for path in &paths {
            remove_recording(path);
        }
        self.conn.execute(delete, params)?;
        Ok(())
    }

    /// Removes one entry and its recording. A missing id is not an error: the
    /// Run Log paints from a snapshot, so two windows deleting the same row must
    /// both succeed. The recording goes too — a transcript is plausibly deleted
    /// *because* it was sensitive.
    pub fn delete(&self, id: i64) -> Result<()> {
        self.unlink_and_delete(
            "SELECT audio_path FROM history WHERE id = ?1 AND audio_path IS NOT NULL",
            "DELETE FROM history WHERE id = ?1",
            rusqlite::params![id],
        )
    }

    /// Replaces the pasted text after a cleanup re-run. `raw_transcript` is left
    /// alone: a cleanup retry never re-derives it.
    pub fn update_final(&self, id: i64, text: &str) -> Result<()> {
        self.update_transcripts(id, None, text)
    }

    /// Writes back a retry. `raw` is `Some` only for a transcription re-run, the
    /// one case that produces new transcriber output. One statement behind both
    /// so the pair cannot drift.
    pub fn update_transcripts(&self, id: i64, raw: Option<&str>, final_text: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE history
             SET final_transcript = ?2, raw_transcript = COALESCE(?3, raw_transcript)
             WHERE id = ?1",
            rusqlite::params![id, final_text, raw],
        )?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.unlink_and_delete(
            "SELECT audio_path FROM history WHERE audio_path IS NOT NULL",
            "DELETE FROM history",
            rusqlite::params![],
        )
    }
}

/// Shared by every read, so a new column cannot reach one query and not another.
const SELECT: &str = "SELECT id, created_at, raw_transcript, final_transcript, app_name,
                             duration_ms, utterance_id, audio_path FROM history";

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        created_at: row.get(1)?,
        raw_transcript: row.get(2)?,
        final_transcript: row.get(3)?,
        app_name: row.get(4)?,
        duration_ms: row.get(5)?,
        utterance_id: row.get(6)?,
        audio_path: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn insert(history: &History, at: i64, text: &str, retention: usize) {
        history
            .insert(
                at,
                text,
                text,
                Some("TestApp"),
                100,
                Some(at),
                None,
                Retention::entries(retention),
            )
            .unwrap();
    }

    fn insert_with_audio(
        history: &History,
        dir: &Path,
        at: i64,
        bytes: usize,
        retention: Retention,
    ) -> PathBuf {
        let wav = dir.join(format!("{at}.wav"));
        std::fs::write(&wav, vec![0u8; bytes]).unwrap();
        history
            .insert(
                at,
                "raw",
                "final",
                None,
                100,
                Some(at),
                Some(wav.to_str().unwrap()),
                retention,
            )
            .unwrap();
        wav
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
    fn delete_removes_only_that_entry() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "keep", 50);
        insert(&history, 200, "drop", 50);

        let doomed = history.recent(10).unwrap()[0].id;
        history.delete(doomed).unwrap();

        let left = history.recent(10).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].final_transcript, "keep");
    }

    #[test]
    fn deleting_a_missing_id_is_not_an_error() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "kept", 50);
        history.delete(9999).unwrap();
        assert_eq!(history.recent(10).unwrap().len(), 1);
    }

    /// Losing `raw_transcript` leaves the run with nothing to retry from.
    #[test]
    fn update_final_leaves_the_raw_transcript_alone() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "um so like the thing", 50);
        let id = history.recent(10).unwrap()[0].id;

        history.update_final(id, "The thing.").unwrap();

        let row = &history.recent(10).unwrap()[0];
        assert_eq!(row.final_transcript, "The thing.");
        assert_eq!(row.raw_transcript, "um so like the thing");
    }

    /// Guards: `utterance_id` added to `CREATE TABLE IF NOT EXISTS` is a no-op
    /// on an existing database, so every upgraded machine failed at the first
    /// SELECT with "no such column".
    #[test]
    fn opens_a_database_written_before_utterance_id_existed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");

        // Byte-for-byte the pre-migration schema, with a row in it.
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE history (
                    id               INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at       INTEGER NOT NULL,
                    raw_transcript   TEXT NOT NULL,
                    final_transcript TEXT NOT NULL,
                    app_name         TEXT,
                    duration_ms      INTEGER NOT NULL DEFAULT 0
                );
                INSERT INTO history (created_at, raw_transcript, final_transcript, duration_ms)
                VALUES (100, 'legacy', 'legacy', 42);",
            )
            .unwrap();
        }

        let history = History::open(&path).unwrap();
        let rows = history.recent(10).unwrap();
        assert_eq!(rows.len(), 1, "the pre-existing row must survive");
        assert_eq!(rows[0].final_transcript, "legacy");
        assert_eq!(rows[0].utterance_id, None, "no breakdown to join to");

        // Reopening must not try to add the column a second time.
        insert(&history, 200, "after", 50);
        drop(history);
        let reopened = History::open(&path).unwrap();
        assert_eq!(reopened.recent(10).unwrap()[0].utterance_id, Some(200));
    }

    /// Guards: a WAV per dictation with nothing evicting them fills the disk.
    #[test]
    fn filling_the_audio_budget_evicts_the_oldest_recordings() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();

        // Three 100-byte recordings into a 250-byte budget: #3 prunes #1.
        let budget = Retention {
            entries: 50,
            audio_bytes: 250,
            audio_days: 0,
        };
        let oldest = insert_with_audio(&history, &media, 100, 100, budget);
        let middle = insert_with_audio(&history, &media, 200, 100, budget);
        let newest = insert_with_audio(&history, &media, 300, 100, budget);

        assert!(!oldest.exists(), "the oldest recording is over budget");
        assert!(middle.exists());
        assert!(newest.exists());

        let rows = history.recent(10).unwrap();
        assert_eq!(rows.len(), 3, "evicting audio must not evict transcripts");
        // Newest first, so the evicted one is last.
        assert_eq!(rows[2].audio_path, None, "a dead path is never left behind");
        assert!(rows[0].audio_path.is_some());
    }

    /// Age evicts independently of size.
    #[test]
    fn recordings_past_the_age_limit_go_even_when_the_size_fits() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();

        let budget = Retention {
            entries: 50,
            audio_bytes: 1_000_000,
            audio_days: 7,
        };
        let now = chrono::Utc::now().timestamp();
        let stale = insert_with_audio(&history, &media, now - 30 * 86_400, 10, budget);
        let fresh = insert_with_audio(&history, &media, now, 10, budget);

        assert!(!stale.exists());
        assert!(fresh.exists());
        assert_eq!(history.recent(10).unwrap().len(), 2);
    }

    /// Guards: pruning the row before its file leaves audio nothing can reach.
    #[test]
    fn deleting_an_entry_takes_its_recording_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();

        let budget = Retention {
            entries: 50,
            audio_bytes: 1_000_000,
            audio_days: 0,
        };
        let wav = insert_with_audio(&history, dir.path(), 100, 10, budget);
        let id = history.recent(10).unwrap()[0].id;

        history.delete(id).unwrap();
        assert!(!wav.exists());
        assert!(history.recent(10).unwrap().is_empty());
    }

    #[test]
    fn clearing_removes_every_recording() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();

        let budget = Retention {
            entries: 50,
            audio_bytes: 1_000_000,
            audio_days: 0,
        };
        let a = insert_with_audio(&history, dir.path(), 100, 10, budget);
        let b = insert_with_audio(&history, dir.path(), 200, 10, budget);

        history.clear().unwrap();
        assert!(!a.exists() && !b.exists());
    }

    /// Losing the file is survivable; losing the path while the file lives is not.
    #[test]
    fn the_entry_cap_unlinks_before_it_forgets_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::open(&dir.path().join("history.sqlite")).unwrap();

        let budget = Retention {
            entries: 2,
            audio_bytes: 1_000_000,
            audio_days: 0,
        };
        let first = insert_with_audio(&history, dir.path(), 100, 10, budget);
        insert_with_audio(&history, dir.path(), 200, 10, budget);
        insert_with_audio(&history, dir.path(), 300, 10, budget);

        assert!(!first.exists(), "the pruned row's recording must go too");
        assert_eq!(history.recent(10).unwrap().len(), 2);
    }

    /// The same "no such column" failure, one schema version on.
    #[test]
    fn opens_a_database_written_before_audio_path_existed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite");

        // Byte-for-byte the user_version = 1 schema, with a row in it.
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE history (
                    id               INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_at       INTEGER NOT NULL,
                    raw_transcript   TEXT NOT NULL,
                    final_transcript TEXT NOT NULL,
                    app_name         TEXT,
                    duration_ms      INTEGER NOT NULL DEFAULT 0,
                    utterance_id     INTEGER
                );
                PRAGMA user_version = 1;
                INSERT INTO history (created_at, raw_transcript, final_transcript, duration_ms)
                VALUES (100, 'legacy', 'legacy', 42);",
            )
            .unwrap();
        }

        let history = History::open(&path).unwrap();
        let rows = history.recent(10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].audio_path, None, "an old run has no recording");
    }

    /// A transcription retry replaces both halves; a cleanup retry only the second.
    #[test]
    fn a_transcription_retry_replaces_the_raw_transcript_too() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "muddled", 50);
        let id = history.recent(10).unwrap()[0].id;

        history
            .update_transcripts(id, Some("clear words"), "Clear words.")
            .unwrap();

        let row = &history.recent(10).unwrap()[0];
        assert_eq!(row.raw_transcript, "clear words");
        assert_eq!(row.final_transcript, "Clear words.");
    }

    #[test]
    fn clear_empties_the_table() {
        let history = History::open_in_memory().unwrap();
        insert(&history, 100, "gone", 50);
        history.clear().unwrap();
        assert!(history.recent(10).unwrap().is_empty());
    }
}
