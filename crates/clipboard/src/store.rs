//! The on-disk side: history entries, snippets and picked colours in one
//! SQLite file, images as PNGs beside it. Local only — nothing here is sent
//! anywhere, logged, or shown to a model.

use anyhow::{Context, Result};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::entry::{self, content_hash, Captured, ClipboardEntry, ClipboardKind};

/// Unpinned entries kept; the oldest go first. Pins are exempt, because the
/// point of pinning is that the cap does not apply.
pub const MAX_ENTRIES: usize = 200;
/// A copy bigger than this is skipped, not truncated: half a document pasted
/// back later is worse than no entry.
pub const MAX_TEXT_BYTES: usize = 200_000;
/// Raw RGBA ceiling. A 5K retina screenshot is ~59 MB; past that the PNG
/// encode would stall the watcher for seconds.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const PREVIEW_CHARS: usize = 300;
const THUMB_EDGE: u32 = 96;

/// A snippet: text kept to be pasted again.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: u32,
    pub name: String,
    /// A short word to find it by ("sig", "addr"). Empty for none.
    pub trigger: String,
    pub body: String,
}

/// A colour the eyedropper picked.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedColor {
    pub id: u32,
    pub hex: String,
    pub picked_at: u32,
}

/// What an entry holds, for the actions that need the real thing.
#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    Image(PathBuf),
    Files(Vec<String>),
}

pub struct Store {
    conn: Mutex<Connection>,
    dir: PathBuf,
}

pub(crate) fn now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
}

fn preview_of(text: &str) -> String {
    let mut preview: String = text.trim().chars().take(PREVIEW_CHARS).collect();
    if text.trim().chars().count() > PREVIEW_CHARS {
        preview.push('…');
    }
    preview
}

fn base_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store> {
        std::fs::create_dir_all(dir.join("img")).context("creating the clipboard folder")?;
        let conn =
            Connection::open(dir.join("clipboard.sqlite")).context("opening clipboard.sqlite")?;
        Self::with(conn, dir)
    }

    fn with(conn: Connection, dir: &Path) -> Result<Store> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                hash TEXT NOT NULL UNIQUE,
                text TEXT,
                files TEXT,
                image TEXT,
                preview TEXT NOT NULL,
                thumb TEXT,
                pinned INTEGER NOT NULL DEFAULT 0,
                copied_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS entries_order ON entries (pinned DESC, copied_at DESC);
            CREATE TABLE IF NOT EXISTS snippets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                trigger TEXT NOT NULL DEFAULT '',
                body TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS colors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hex TEXT NOT NULL UNIQUE,
                picked_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Store {
            conn: Mutex::new(conn),
            dir: dir.to_path_buf(),
        })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        // The watcher thread and the commands share this; one panic while
        // holding it must not take the history down with it.
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// History is off until the user turns it on: a clipboard is where
    /// passwords and private messages pass through.
    pub fn enabled(&self) -> bool {
        self.db()
            .query_row("SELECT value FROM meta WHERE key = 'enabled'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .ok()
            .flatten()
            .is_some_and(|v| v == "1")
    }

    pub fn set_enabled(&self, on: bool) -> Result<()> {
        self.db().execute(
            "INSERT INTO meta (key, value) VALUES ('enabled', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [if on { "1" } else { "0" }],
        )?;
        Ok(())
    }

    /// Stores a copy, or moves the existing entry for the same content to the
    /// top. Returns None when the copy is empty or over the size ceilings.
    pub fn record(&self, captured: Captured) -> Result<Option<u32>> {
        let row = match captured {
            Captured::Text(text) => {
                if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
                    return Ok(None);
                }
                let hash = content_hash(ClipboardKind::Text, &[text.as_bytes()]);
                NewRow {
                    kind: ClipboardKind::Text,
                    hash,
                    preview: preview_of(&text),
                    text: Some(text),
                    files: None,
                    image: None,
                    thumb: None,
                }
            }
            Captured::Files(files) => {
                if files.is_empty() {
                    return Ok(None);
                }
                let joined = files.join("\n");
                let names: Vec<&str> = files.iter().map(|f| base_name(f)).collect();
                NewRow {
                    kind: ClipboardKind::Files,
                    hash: content_hash(ClipboardKind::Files, &[joined.as_bytes()]),
                    preview: preview_of(&names.join(", ")),
                    text: None,
                    files: Some(serde_json::to_string(&files)?),
                    image: None,
                    thumb: None,
                }
            }
            Captured::Image {
                width,
                height,
                rgba,
            } => {
                if rgba.is_empty() || rgba.len() > MAX_IMAGE_BYTES {
                    return Ok(None);
                }
                let dims = format!("{width}x{height}");
                let hash = content_hash(ClipboardKind::Image, &[dims.as_bytes(), &rgba]);
                // Skip the encode when this exact image is already stored.
                if let Some(id) = self.bump(&hash)? {
                    return Ok(Some(id));
                }
                let path = self.dir.join("img").join(format!("{hash}.png"));
                let thumb = encode_images(&path, width, height, rgba)?;
                NewRow {
                    kind: ClipboardKind::Image,
                    hash,
                    preview: format!("Image · {width}×{height}"),
                    text: None,
                    files: None,
                    image: Some(path.to_string_lossy().into_owned()),
                    thumb: Some(thumb),
                }
            }
        };
        if let Some(id) = self.bump(&row.hash)? {
            return Ok(Some(id));
        }
        let id = {
            let db = self.db();
            db.execute(
                "INSERT INTO entries (kind, hash, text, files, image, preview, thumb, copied_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    row.kind.tag(),
                    row.hash,
                    row.text,
                    row.files,
                    row.image,
                    row.preview,
                    row.thumb,
                    now()
                ],
            )?;
            u32::try_from(db.last_insert_rowid()).unwrap_or(u32::MAX)
        };
        self.evict()?;
        Ok(Some(id))
    }

    /// A copy of something already stored is a re-copy: it surfaces, it does
    /// not duplicate.
    fn bump(&self, hash: &str) -> Result<Option<u32>> {
        let db = self.db();
        db.execute(
            "UPDATE entries SET copied_at = ?1 WHERE hash = ?2",
            params![now(), hash],
        )?;
        Ok(db
            .query_row("SELECT id FROM entries WHERE hash = ?1", [hash], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// Drops unpinned entries past the cap, oldest first, and their images.
    fn evict(&self) -> Result<()> {
        let doomed: Vec<(u32, Option<String>)> = {
            let db = self.db();
            let mut stmt = db.prepare(
                "SELECT id, image FROM entries WHERE pinned = 0
                 ORDER BY copied_at DESC, id DESC LIMIT -1 OFFSET ?1",
            )?;
            let rows = stmt.query_map([MAX_ENTRIES as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, image) in doomed {
            self.remove(id, image.as_deref())?;
        }
        Ok(())
    }

    fn remove(&self, id: u32, image: Option<&str>) -> Result<()> {
        self.db()
            .execute("DELETE FROM entries WHERE id = ?1", [id])?;
        if let Some(path) = image {
            // Already gone is fine: the row is what mattered.
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    /// Entries, pinned first then newest, filtered by `query` (a substring of
    /// the text, file names or kind).
    pub fn list(&self, query: &str, limit: u32) -> Result<Vec<ClipboardEntry>> {
        let db = self.db();
        let like = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = db.prepare(
            "SELECT id, kind, text, files, preview, thumb, pinned, copied_at FROM entries
             WHERE ?1 = '%%' OR text LIKE ?1 ESCAPE '\\' OR preview LIKE ?1 ESCAPE '\\'
             ORDER BY pinned DESC, copied_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![like, limit], |r| {
            let kind = ClipboardKind::from_tag(&r.get::<_, String>(1)?);
            let text: Option<String> = r.get(2)?;
            let actions = match kind {
                ClipboardKind::Text => entry::text_actions(text.as_deref().unwrap_or_default()),
                ClipboardKind::Files => entry::file_actions(),
                ClipboardKind::Image => entry::image_actions(),
            };
            Ok(ClipboardEntry {
                id: r.get(0)?,
                kind,
                preview: r.get(4)?,
                thumbnail: r.get(5)?,
                pinned: r.get::<_, i64>(6)? != 0,
                copied_at: r.get(7)?,
                actions,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn content(&self, id: u32) -> Result<Option<Content>> {
        type Row = (String, Option<String>, Option<String>, Option<String>);
        let row: Option<Row> = self
            .db()
            .query_row(
                "SELECT kind, text, files, image FROM entries WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        Ok(row.map(
            |(kind, text, files, image)| match ClipboardKind::from_tag(&kind) {
                ClipboardKind::Text => Content::Text(text.unwrap_or_default()),
                ClipboardKind::Image => Content::Image(PathBuf::from(image.unwrap_or_default())),
                ClipboardKind::Files => Content::Files(
                    files
                        .and_then(|f| serde_json::from_str(&f).ok())
                        .unwrap_or_default(),
                ),
            },
        ))
    }

    pub fn pin(&self, id: u32, pinned: bool) -> Result<()> {
        self.db().execute(
            "UPDATE entries SET pinned = ?1 WHERE id = ?2",
            params![i64::from(pinned), id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: u32) -> Result<()> {
        let image: Option<String> = self
            .db()
            .query_row("SELECT image FROM entries WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .flatten();
        self.remove(id, image.as_deref())
    }

    /// Clears the history. Pins stay: clearing is "forget what I copied", not
    /// "undo what I chose to keep".
    pub fn clear(&self) -> Result<()> {
        let images: Vec<String> = {
            let db = self.db();
            let mut stmt =
                db.prepare("SELECT image FROM entries WHERE pinned = 0 AND image IS NOT NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        self.db()
            .execute("DELETE FROM entries WHERE pinned = 0", [])?;
        for path in images {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn snippets(&self) -> Result<Vec<Snippet>> {
        let db = self.db();
        let mut stmt = db
            .prepare("SELECT id, name, trigger, body FROM snippets ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(Snippet {
                id: r.get(0)?,
                name: r.get(1)?,
                trigger: r.get(2)?,
                body: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Creates a snippet, or edits the one with `id`.
    pub fn save_snippet(
        &self,
        id: Option<u32>,
        name: &str,
        trigger: &str,
        body: &str,
    ) -> Result<u32> {
        let (name, trigger) = (name.trim(), trigger.trim());
        anyhow::ensure!(!name.is_empty(), "a snippet needs a name");
        anyhow::ensure!(!body.is_empty(), "a snippet needs some text");
        let db = self.db();
        match id {
            Some(id) => {
                let changed = db.execute(
                    "UPDATE snippets SET name = ?1, trigger = ?2, body = ?3 WHERE id = ?4",
                    params![name, trigger, body, id],
                )?;
                anyhow::ensure!(changed == 1, "that snippet no longer exists");
                Ok(id)
            }
            None => {
                db.execute(
                    "INSERT INTO snippets (name, trigger, body) VALUES (?1, ?2, ?3)",
                    params![name, trigger, body],
                )?;
                Ok(u32::try_from(db.last_insert_rowid()).unwrap_or(u32::MAX))
            }
        }
    }

    pub fn snippet_body(&self, id: u32) -> Result<Option<String>> {
        Ok(self
            .db()
            .query_row("SELECT body FROM snippets WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn delete_snippet(&self, id: u32) -> Result<()> {
        self.db()
            .execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Notes a picked colour; a repeat moves it to the front.
    pub fn record_color(&self, hex: &str) -> Result<()> {
        let db = self.db();
        db.execute(
            "INSERT INTO colors (hex, picked_at) VALUES (?1, ?2)
             ON CONFLICT(hex) DO UPDATE SET picked_at = excluded.picked_at",
            params![hex, now()],
        )?;
        // ponytail: a fixed 30 swatches. A "keep this one" flag is the upgrade
        // if people want a palette that outlives new picks.
        db.execute(
            "DELETE FROM colors WHERE id NOT IN (SELECT id FROM colors ORDER BY picked_at DESC, id DESC LIMIT 30)",
            [],
        )?;
        Ok(())
    }

    pub fn colors(&self) -> Result<Vec<PickedColor>> {
        let db = self.db();
        let mut stmt =
            db.prepare("SELECT id, hex, picked_at FROM colors ORDER BY picked_at DESC, id DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok(PickedColor {
                id: r.get(0)?,
                hex: r.get(1)?,
                picked_at: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn clear_colors(&self) -> Result<()> {
        self.db().execute("DELETE FROM colors", [])?;
        Ok(())
    }
}

struct NewRow {
    kind: ClipboardKind,
    hash: String,
    preview: String,
    text: Option<String>,
    files: Option<String>,
    image: Option<String>,
    thumb: Option<String>,
}

/// Writes the full PNG to `path` and returns a small PNG as a `data:` URI.
fn encode_images(path: &Path, width: usize, height: usize, rgba: Vec<u8>) -> Result<String> {
    let (w, h) = (u32::try_from(width)?, u32::try_from(height)?);
    let full =
        image::RgbaImage::from_raw(w, h, rgba).context("image bytes do not match its size")?;
    full.save_with_format(path, image::ImageFormat::Png)
        .with_context(|| format!("writing {}", path.display()))?;
    let thumb = image::imageops::thumbnail(
        &full,
        THUMB_EDGE.min(w).max(1),
        (THUMB_EDGE * h / w.max(1)).clamp(1, THUMB_EDGE),
    );
    let mut png = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.into_inner())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Store::open(dir.path()).unwrap(), dir)
    }

    fn text(s: &str) -> Captured {
        Captured::Text(s.to_string())
    }

    #[test]
    fn history_is_off_until_turned_on() {
        let (store, _dir) = store();
        assert!(!store.enabled());
        store.set_enabled(true).unwrap();
        assert!(store.enabled());
    }

    #[test]
    fn copying_the_same_thing_again_surfaces_it_instead_of_duplicating() {
        let (store, _dir) = store();
        let a = store.record(text("alpha")).unwrap().unwrap();
        store.record(text("beta")).unwrap();
        store
            .db()
            .execute(
                "UPDATE entries SET copied_at = 100 WHERE text = 'alpha'",
                [],
            )
            .unwrap();
        store
            .db()
            .execute("UPDATE entries SET copied_at = 200 WHERE text = 'beta'", [])
            .unwrap();
        assert_eq!(store.record(text("alpha")).unwrap(), Some(a));
        let list = store.list("", 10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a, "the re-copy is on top");
    }

    #[test]
    fn empty_and_oversized_copies_are_skipped() {
        let (store, _dir) = store();
        assert_eq!(store.record(text("   \n")).unwrap(), None);
        assert_eq!(
            store.record(text(&"x".repeat(MAX_TEXT_BYTES + 1))).unwrap(),
            None
        );
        assert!(store.list("", 10).unwrap().is_empty());
    }

    #[test]
    fn the_cap_drops_the_oldest_unpinned_and_never_a_pin() {
        let (store, _dir) = store();
        let pinned = store.record(text("keep me")).unwrap().unwrap();
        store.pin(pinned, true).unwrap();
        for i in 0..MAX_ENTRIES + 5 {
            store.record(text(&format!("entry {i}"))).unwrap();
        }
        // Make "keep me" the oldest of all, so only the pin saves it.
        store
            .db()
            .execute("UPDATE entries SET copied_at = 1 WHERE id = ?1", [pinned])
            .unwrap();
        store.record(text("one more")).unwrap();
        let all = store.list("", 1000).unwrap();
        assert_eq!(all.iter().filter(|e| !e.pinned).count(), MAX_ENTRIES);
        assert!(all.iter().any(|e| e.id == pinned));
        assert!(all[0].pinned, "pins list first");
        assert!(
            !all.iter().any(|e| e.preview == "entry 0"),
            "the oldest went"
        );
    }

    #[test]
    fn clear_forgets_history_but_keeps_pins_and_removes_image_files() {
        let (store, dir) = store();
        let keep = store.record(text("pinned")).unwrap().unwrap();
        store.pin(keep, true).unwrap();
        store.record(text("gone")).unwrap();
        let rgba = vec![200u8; 4 * 8 * 8];
        let img = store
            .record(Captured::Image {
                width: 8,
                height: 8,
                rgba,
            })
            .unwrap()
            .unwrap();
        let Some(Content::Image(path)) = store.content(img).unwrap() else {
            panic!("not an image")
        };
        assert!(path.exists());
        store.clear().unwrap();
        assert!(!path.exists(), "the image file goes with its row");
        let left = store.list("", 10).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, keep);
        drop(dir);
    }

    #[test]
    fn an_image_gets_a_thumbnail_and_a_dimensions_preview() {
        let (store, _dir) = store();
        let rgba = vec![90u8; 4 * 200 * 100];
        store
            .record(Captured::Image {
                width: 200,
                height: 100,
                rgba,
            })
            .unwrap();
        let e = &store.list("", 10).unwrap()[0];
        assert_eq!(e.kind, ClipboardKind::Image);
        assert_eq!(e.preview, "Image · 200×100");
        assert!(e
            .thumbnail
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn search_matches_text_and_treats_percent_literally() {
        let (store, _dir) = store();
        store.record(text("invoice 100% paid")).unwrap();
        store.record(text("hello world")).unwrap();
        assert_eq!(store.list("INVOICE", 10).unwrap().len(), 1);
        assert_eq!(
            store.list("%", 10).unwrap().len(),
            1,
            "a % is not a wildcard"
        );
        assert_eq!(store.list("zzz", 10).unwrap().len(), 0);
    }

    #[test]
    fn snippets_need_a_name_and_text_and_edit_in_place() {
        let (store, _dir) = store();
        assert!(store.save_snippet(None, " ", "", "body").is_err());
        assert!(store.save_snippet(None, "Sig", "", "").is_err());
        let id = store
            .save_snippet(None, "Sig", " sig ", "Best,\nYaksh")
            .unwrap();
        store
            .save_snippet(Some(id), "Signature", "sig", "Best,\nY")
            .unwrap();
        let all = store.snippets().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            (all[0].name.as_str(), all[0].trigger.as_str()),
            ("Signature", "sig")
        );
        assert!(store.save_snippet(Some(999), "x", "", "y").is_err());
        store.delete_snippet(id).unwrap();
        assert!(store.snippet_body(id).unwrap().is_none());
    }

    #[test]
    fn a_repeated_colour_moves_to_the_front_and_the_list_is_capped() {
        let (store, _dir) = store();
        store.record_color("#111111").unwrap();
        store
            .db()
            .execute("UPDATE colors SET picked_at = 1", [])
            .unwrap();
        store.record_color("#222222").unwrap();
        store
            .db()
            .execute("UPDATE colors SET picked_at = 5 WHERE hex = '#222222'", [])
            .unwrap();
        store.record_color("#111111").unwrap();
        assert_eq!(store.colors().unwrap()[0].hex, "#111111");
        for i in 0..40 {
            store.record_color(&format!("#{i:06x}")).unwrap();
        }
        assert_eq!(store.colors().unwrap().len(), 30);
    }
}
