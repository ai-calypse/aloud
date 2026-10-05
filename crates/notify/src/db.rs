//! Notification Center's database: where it is, whether this build can read
//! it, and "what is new since the last row I saw".
//!
//! Evidence for what is read, all from public forensic write-ups rather than
//! Apple (there is no Apple documentation): `usernoted` keeps SQLite 3 at
//! `~/Library/Group Containers/group.com.apple.usernoted/db2/db` from macOS 15
//! (moved there from `$DARWIN_USER_DIR/com.apple.notificationcenter/db2/db`
//! so that TCC guards it). Tables `app(app_id, identifier)` and
//! `record(rec_id, app_id, uuid, data, request_date, request_last_date,
//! delivered_date, presented, style, snooze_fire_date)`; `data` is a binary
//! plist whose `req` dictionary holds `titl`, `subt` and `body`, and which
//! repeats the bundle id under `app` (Objective-See's "notification
//! database" post; plaso's `mac_notificationcenter` parser).

use std::fmt;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::MirrorState;

/// One notification. Its text is for the notch and then gone; see the crate docs.
#[derive(Clone, PartialEq, Eq)]
pub struct Notification {
    /// The row, for a stable island id.
    pub rec_id: i64,
    /// The sending app's bundle id.
    pub app: String,
    pub title: String,
    pub subtitle: String,
    pub body: String,
}

impl Notification {
    /// Whether any of the three says `word`, ignoring case.
    pub(crate) fn mentions(&self, word: &str) -> bool {
        [&self.title, &self.subtitle, &self.body]
            .iter()
            .any(|s| s.to_lowercase().contains(word))
    }
}

/// The app and the row, never the words: a stray `{:?}` in a log line must not
/// be a way for a message to reach a file.
impl fmt::Debug for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Notification")
            .field("app", &self.app)
            .field("rec_id", &self.rec_id)
            .finish_non_exhaustive()
    }
}

/// Where the database may be, newest layout first.
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(
            Path::new(&home).join("Library/Group Containers/group.com.apple.usernoted/db2/db"),
        );
    }
    if let Some(dir) = darwin_user_dir() {
        paths.push(dir.join("com.apple.notificationcenter/db2/db"));
    }
    paths
}

/// `getconf DARWIN_USER_DIR`: the per-user temp root macOS 14 and older kept
/// the database under. A spawned `getconf` because there is no stable Rust API
/// for `confstr`, and this runs once per start.
fn darwin_user_dir() -> Option<PathBuf> {
    let out = std::process::Command::new("getconf")
        .arg("DARWIN_USER_DIR")
        .output()
        .ok()?;
    let dir = String::from_utf8(out.stdout).ok()?;
    let dir = dir.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

/// The first database that exists. A permission error is not "missing": under
/// TCC an unreadable container answers `EPERM` to a `stat`, which is exactly
/// how a Mac without Full Disk Access tells the two apart.
pub(crate) fn locate(paths: &[PathBuf]) -> Result<PathBuf, MirrorState> {
    let mut denied = false;
    for path in paths {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_file() => return Ok(path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => denied = true,
            _ => {}
        }
    }
    Err(if denied {
        MirrorState::NeedsAccess
    } else {
        MirrorState::Unsupported {
            detail: "no Notification Center database at the known locations".into(),
        }
    })
}

/// A SQLite failure as the state it means. SQLite reports a TCC denial as
/// "unable to open database file", the same code as a missing file; by now the
/// file was seen to exist, so it is the permission.
fn classify(e: &rusqlite::Error) -> MirrorState {
    match e.sqlite_error_code() {
        Some(ErrorCode::CannotOpen | ErrorCode::PermissionDenied | ErrorCode::ReadOnly) => {
            MirrorState::NeedsAccess
        }
        _ => MirrorState::Unsupported {
            detail: e.to_string(),
        },
    }
}

const NEEDED: [(&str, &[&str]); 2] = [
    ("app", &["app_id", "identifier"]),
    ("record", &["rec_id", "app_id", "data"]),
];

/// An open, read-only connection and the newest row already seen.
pub struct Reader {
    conn: Connection,
    last: i64,
}

impl Reader {
    /// Opens the database for this Mac, baselined at its newest row so that
    /// starting the mirror never replays old notifications.
    pub fn open() -> Result<Self, MirrorState> {
        Self::open_at(&locate(&candidates())?)
    }

    pub fn open_at(path: &Path) -> Result<Self, MirrorState> {
        // Read-only, and not `immutable`: usernoted is writing this file now.
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| classify(&e))?;
        conn.busy_timeout(Duration::from_secs(1))
            .map_err(|e| classify(&e))?;
        check_schema(&conn)?;
        let last = max_id(&conn).map_err(|e| classify(&e))?;
        Ok(Self { conn, last })
    }

    /// Notifications that arrived since the last call, oldest first.
    ///
    /// A row whose plist does not parse, or has no words, still moves the
    /// cursor: it would never parse later, and re-reading it every second
    /// would be a busy loop.
    pub fn poll(&mut self) -> Result<Vec<Notification>, MirrorState> {
        let max = max_id(&self.conn).map_err(|e| classify(&e))?;
        if max < self.last {
            // Rows were deleted from the top (the user cleared Notification
            // Center) and ids restart from the new top: start from there, or
            // every notification until the old number is silently lost.
            self.last = max;
            return Ok(Vec::new());
        }
        if max == self.last {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare_cached(
                "SELECT r.rec_id, a.identifier, r.data \
                 FROM record r LEFT JOIN app a ON a.app_id = r.app_id \
                 WHERE r.rec_id > ?1 ORDER BY r.rec_id LIMIT 50",
            )
            .map_err(|e| classify(&e))?;
        let rows = stmt
            .query_map([self.last], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                ))
            })
            .map_err(|e| classify(&e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| classify(&e))?;
        let mut fresh = Vec::new();
        for (rec_id, identifier, data) in rows {
            self.last = rec_id;
            if let Some(n) = data.and_then(|d| parse(rec_id, identifier, &d)) {
                fresh.push(n);
            }
        }
        Ok(fresh)
    }
}

fn max_id(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(rec_id), 0) FROM record", [], |r| {
        r.get(0)
    })
}

fn check_schema(conn: &Connection) -> Result<(), MirrorState> {
    for (table, columns) in NEEDED {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| classify(&e))?;
        let have: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| classify(&e))?
            .filter_map(Result::ok)
            .collect();
        if let Some(missing) = columns.iter().find(|c| !have.iter().any(|h| h == *c)) {
            return Err(MirrorState::Unsupported {
                detail: format!("table `{table}` has no column `{missing}`"),
            });
        }
    }
    Ok(())
}

/// One row's plist as a notification; None when it has nothing to say.
pub(crate) fn parse(rec_id: i64, identifier: Option<String>, data: &[u8]) -> Option<Notification> {
    let root = plist::Value::from_reader(Cursor::new(data)).ok()?;
    let root = root.as_dictionary()?;
    let req = root.get("req")?.as_dictionary()?;
    let text = |key: &str| {
        req.get(key)
            .and_then(plist::Value::as_string)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let (title, subtitle, body) = (text("titl"), text("subt"), text("body"));
    if title.is_empty() && subtitle.is_empty() && body.is_empty() {
        return None;
    }
    let app = identifier.filter(|s| !s.is_empty()).or_else(|| {
        root.get("app")
            .and_then(plist::Value::as_string)
            .map(str::to_owned)
    })?;
    Some(Notification {
        rec_id,
        app,
        title,
        subtitle,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::{Dictionary, Value};

    /// A row's `data` the way usernoted writes it: a binary plist.
    fn blob(app: &str, title: &str, subtitle: &str, body: &str) -> Vec<u8> {
        let mut req = Dictionary::new();
        req.insert("titl".into(), title.into());
        req.insert("subt".into(), subtitle.into());
        req.insert("body".into(), body.into());
        let mut root = Dictionary::new();
        root.insert("app".into(), app.into());
        root.insert("req".into(), Value::Dictionary(req));
        let mut out = Vec::new();
        plist::to_writer_binary(&mut out, &Value::Dictionary(root)).unwrap();
        out
    }

    /// Notification Center's schema as the public write-ups give it.
    fn fixture(dir: &Path) -> (PathBuf, Connection) {
        let path = dir.join("db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app (app_id INTEGER PRIMARY KEY, identifier VARCHAR);
             CREATE TABLE record (rec_id INTEGER PRIMARY KEY, app_id INTEGER, uuid BLOB,
               data BLOB, request_date REAL, request_last_date REAL, delivered_date REAL,
               presented INTEGER, style INTEGER, snooze_fire_date REAL);
             INSERT INTO app VALUES (1, 'com.apple.MobileSMS'), (2, 'com.tinyspeck.slackmacgap');",
        )
        .unwrap();
        (path, conn)
    }

    fn add(conn: &Connection, rec_id: i64, app_id: i64, data: &[u8]) {
        conn.execute(
            "INSERT INTO record (rec_id, app_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![rec_id, app_id, data],
        )
        .unwrap();
    }

    #[test]
    fn starting_never_replays_what_was_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let (path, conn) = fixture(dir.path());
        add(
            &conn,
            1,
            1,
            &blob("com.apple.MobileSMS", "Old", "", "from yesterday"),
        );
        let mut reader = Reader::open_at(&path).unwrap();
        assert!(reader.poll().unwrap().is_empty());
    }

    #[test]
    fn a_new_row_is_returned_once_with_its_app_and_words() {
        let dir = tempfile::tempdir().unwrap();
        let (path, conn) = fixture(dir.path());
        let mut reader = Reader::open_at(&path).unwrap();
        add(
            &conn,
            1,
            2,
            &blob("com.tinyspeck.slackmacgap", "Priya", "#design", "look?"),
        );
        let fresh = reader.poll().unwrap();
        assert_eq!(fresh.len(), 1);
        let n = &fresh[0];
        assert_eq!(
            (
                n.app.as_str(),
                n.title.as_str(),
                n.subtitle.as_str(),
                n.body.as_str()
            ),
            ("com.tinyspeck.slackmacgap", "Priya", "#design", "look?")
        );
        assert!(
            reader.poll().unwrap().is_empty(),
            "the same row is not sent twice"
        );
    }

    #[test]
    fn a_row_that_will_never_parse_is_skipped_once_not_retried_every_second() {
        let dir = tempfile::tempdir().unwrap();
        let (path, conn) = fixture(dir.path());
        let mut reader = Reader::open_at(&path).unwrap();
        add(&conn, 1, 1, b"not a plist");
        add(&conn, 2, 1, &blob("com.apple.MobileSMS", "", "", ""));
        add(&conn, 3, 1, &blob("com.apple.MobileSMS", "Maya", "", "hi"));
        let fresh = reader.poll().unwrap();
        assert_eq!(fresh.iter().map(|n| n.rec_id).collect::<Vec<_>>(), [3]);
        assert!(reader.poll().unwrap().is_empty());
    }

    #[test]
    fn ids_that_restart_after_the_user_clears_notifications_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let (path, conn) = fixture(dir.path());
        add(&conn, 40, 1, &blob("com.apple.MobileSMS", "A", "", "a"));
        let mut reader = Reader::open_at(&path).unwrap();
        conn.execute("DELETE FROM record", []).unwrap();
        assert!(reader.poll().unwrap().is_empty());
        add(&conn, 1, 1, &blob("com.apple.MobileSMS", "B", "", "b"));
        assert_eq!(
            reader.poll().unwrap().len(),
            1,
            "row 1 is new, though 40 was seen"
        );
    }

    #[test]
    fn the_app_falls_back_to_the_plist_when_the_join_finds_none() {
        let dir = tempfile::tempdir().unwrap();
        let (path, conn) = fixture(dir.path());
        let mut reader = Reader::open_at(&path).unwrap();
        add(&conn, 1, 99, &blob("com.example.thing", "T", "", "b"));
        assert_eq!(reader.poll().unwrap()[0].app, "com.example.thing");
    }

    #[test]
    fn a_schema_without_the_columns_is_unsupported_and_says_which() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE app (app_id INTEGER PRIMARY KEY, identifier VARCHAR);
             CREATE TABLE record (rec_id INTEGER PRIMARY KEY, app_id INTEGER, payload BLOB);",
        )
        .unwrap();
        match Reader::open_at(&path) {
            Err(MirrorState::Unsupported { detail }) => {
                assert!(detail.contains("data"), "{detail}")
            }
            other => panic!("{:?}", other.err()),
        }
    }

    #[test]
    fn a_file_that_is_not_a_database_is_unsupported_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        std::fs::write(&path, vec![7u8; 4096]).unwrap();
        assert!(matches!(
            Reader::open_at(&path),
            Err(MirrorState::Unsupported { .. })
        ));
    }

    #[test]
    fn a_missing_database_is_unsupported_and_an_unreadable_folder_needs_access() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nowhere/db");
        assert!(matches!(
            locate(&[missing]),
            Err(MirrorState::Unsupported { .. })
        ));

        // What TCC does to a stat inside the group container.
        use std::os::unix::fs::PermissionsExt;
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("db"), b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied = locate(&[locked.join("db")]);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        // Root ignores mode bits, and then the file simply is found.
        assert!(matches!(denied, Err(MirrorState::NeedsAccess) | Ok(_)));
    }
}
