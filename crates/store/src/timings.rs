//! Per-stage latency, recorded straight off `tracing` spans.
//!
//! A [`TimingLayer`] writes one row per closed span into `timings.sqlite`.
//! Spans only get recorded if they descend from a span carrying an
//! `utterance_id` field, which is what makes the rows reassemble into a
//! waterfall instead of a pile of unrelated durations.
//!
//! ponytail: local-only, no aggregation. `tracing-opentelemetry` composes with
//! this layer if cross-user percentiles are ever wanted.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::span::{Attributes, Id, Record};
use tracing::Subscriber;
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Utterances kept before the oldest are pruned. Sized for days of use: a heavy
/// dictation day alone runs past a couple of hundred, and rows are tiny.
const RETAIN_UTTERANCES: i64 = 5000;

/// Hands out utterance ids, seeded from the database on open: an in-process
/// counter restarted at 0 each launch and reissued ids, merging unrelated runs
/// into one waterfall.
static NEXT_UTTERANCE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// The next unused utterance id. Monotonic across restarts, thanks to the seed.
pub fn next_utterance_id() -> i64 {
    NEXT_UTTERANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// One closed span.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    /// Groups the stages of one dictation; monotonic, so it orders by time.
    #[specta(type = specta_typescript::Number)]
    pub utterance_id: i64,
    /// The span name: `utterance`, `transcribe`, `cleanup`, …
    pub stage: String,
    /// Unix milliseconds at span entry — the waterfall's x offset.
    #[specta(type = specta_typescript::Number)]
    pub started_at: i64,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: i64,
    pub success: bool,
    /// The span's fields, as JSON.
    pub fields: String,
}

pub struct Timings {
    conn: Connection,
}

impl Timings {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::migrate(&conn)?;
        let db = Self { conn };
        db.seed_utterance_ids()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    /// Resumes the id sequence past whatever is on disk. Only ever moves the
    /// counter forward, so a second `open` cannot walk it back.
    fn seed_utterance_ids(&self) -> Result<()> {
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(utterance_id), -1) + 1 FROM timings",
            [],
            |row| row.get(0),
        )?;
        NEXT_UTTERANCE.fetch_max(next, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS timings (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                utterance_id INTEGER NOT NULL,
                stage        TEXT    NOT NULL,
                started_at   INTEGER NOT NULL,
                duration_ms  INTEGER NOT NULL,
                success      INTEGER NOT NULL DEFAULT 1,
                fields       TEXT    NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS timings_utterance ON timings (utterance_id DESC);",
        )
        .context("creating timings schema")?;
        Ok(())
    }

    fn insert(&self, span: &Span) -> Result<()> {
        self.conn.execute(
            "INSERT INTO timings (utterance_id, stage, started_at, duration_ms, success, fields)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                span.utterance_id,
                span.stage,
                span.started_at,
                span.duration_ms,
                span.success,
                span.fields,
            ],
        )?;
        Ok(())
    }

    fn prune(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM timings WHERE utterance_id <= (
                 SELECT MAX(utterance_id) - ?1 FROM timings
             )",
            [RETAIN_UTTERANCES],
        )?;
        Ok(())
    }

    /// The newest `utterances` utterances, oldest stage first within each so the
    /// caller can lay them out directly.
    pub fn recent(&self, utterances: usize) -> Result<Vec<Span>> {
        let mut stmt = self.conn.prepare(
            "SELECT utterance_id, stage, started_at, duration_ms, success, fields
             FROM timings
             WHERE utterance_id > (SELECT MAX(utterance_id) - ?1 FROM timings)
             ORDER BY utterance_id DESC, started_at ASC",
        )?;
        let rows = stmt
            .query_map([utterances as i64], |row| {
                Ok(Span {
                    utterance_id: row.get(0)?,
                    stage: row.get(1)?,
                    started_at: row.get(2)?,
                    duration_ms: row.get(3)?,
                    success: row.get(4)?,
                    fields: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

struct Timing {
    utterance_id: i64,
    started: Instant,
    started_at: i64,
    fields: serde_json::Map<String, serde_json::Value>,
}

/// Writes closed spans to SQLite. Attach with
/// `tracing_subscriber::registry().with(TimingLayer::new(db))`.
pub struct TimingLayer {
    // ponytail: one mutex over the whole connection. A dictation closes ~5
    // spans, so contention is theoretical; a channel to a writer task is the
    // upgrade path if this ever traces something hot.
    db: Arc<Mutex<Timings>>,
}

impl TimingLayer {
    pub fn new(db: Arc<Mutex<Timings>>) -> Self {
        Self { db }
    }
}

impl<S> Layer<S> for TimingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: LayerContext<'_, S>) {
        let mut fields = serde_json::Map::new();
        attrs.record(&mut JsonVisitor(&mut fields));

        // A span joins the trace by declaring an `utterance_id` or descending
        // from one that did; anything else is ordinary logging.
        let utterance_id = match fields.remove("utterance_id").and_then(|v| v.as_i64()) {
            Some(root) => root,
            None => {
                let parent = attrs
                    .parent()
                    .and_then(|p| ctx.span(p))
                    .or_else(|| ctx.lookup_current());
                match parent.and_then(|p| p.extensions().get::<Timing>().map(|t| t.utterance_id)) {
                    Some(id) => id,
                    None => return,
                }
            }
        };

        let Some(span) = ctx.span(id) else { return };
        span.extensions_mut().insert(Timing {
            utterance_id,
            started: Instant::now(),
            started_at: chrono::Utc::now().timestamp_millis(),
            fields,
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: LayerContext<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut ext = span.extensions_mut();
        if let Some(timing) = ext.get_mut::<Timing>() {
            values.record(&mut JsonVisitor(&mut timing.fields));
        }
    }

    fn on_close(&self, id: Id, ctx: LayerContext<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let Some(timing) = span.extensions_mut().remove::<Timing>() else {
            return;
        };
        let record = Span {
            utterance_id: timing.utterance_id,
            stage: span.name().to_string(),
            started_at: timing.started_at,
            duration_ms: timing.started.elapsed().as_millis() as i64,
            // The call site sets `ok = false` on the way out of a failed stage.
            success: timing
                .fields
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            fields: serde_json::Value::Object(timing.fields).to_string(),
        };

        let Ok(db) = self.db.lock() else { return };
        // Tracing must never take the app down with it.
        if let Err(e) = db.insert(&record).and_then(|_| db.prune()) {
            eprintln!("timings: {e:#}");
        }
    }
}

/// Flattens span fields into JSON; `tracing` has no typed field access.
struct JsonVisitor<'a>(&'a mut serde_json::Map<String, serde_json::Value>);

impl tracing::field::Visit for JsonVisitor<'_> {
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_f64(&mut self, f: &tracing::field::Field, v: f64) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.0.insert(f.name().into(), v.into());
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        self.0.insert(f.name().into(), format!("{v:?}").into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    /// Guards: an in-process id counter over on-disk rows reissued ids after a
    /// restart, and two unrelated runs became one waterfall.
    #[test]
    fn utterance_ids_do_not_restart_with_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timings.sqlite");

        {
            let db = Timings::open(&path).unwrap();
            let subscriber = tracing_subscriber::registry().with(TimingLayer::new(Arc::new(
                Mutex::new(Timings::open(&path).unwrap()),
            )));
            tracing::subscriber::with_default(subscriber, || {
                for id in 0..4i64 {
                    let _s = tracing::info_span!("utterance", utterance_id = id).entered();
                }
            });
            assert_eq!(db.recent(20).unwrap().len(), 4);
        }

        // "Restart": a fresh open against the same file.
        let reopened = Timings::open(&path).unwrap();
        assert!(
            next_utterance_id() >= 4,
            "id reissued over an existing run — waterfalls would merge"
        );
        assert_eq!(reopened.recent(20).unwrap().len(), 4, "data must survive");
    }

    /// Correlation and nesting are the point, and neither shows up in the SQL.
    #[test]
    fn nested_spans_share_the_root_utterance_id() {
        let db = Arc::new(Mutex::new(Timings::open_in_memory().unwrap()));
        let subscriber = tracing_subscriber::registry().with(TimingLayer::new(db.clone()));

        tracing::subscriber::with_default(subscriber, || {
            {
                let root = tracing::info_span!("utterance", utterance_id = 7i64);
                let _r = root.enter();
                {
                    let child = tracing::info_span!("transcribe", provider = "groq");
                    let _c = child.enter();
                }
                let failed = tracing::info_span!("paste", ok = tracing::field::Empty);
                let _f = failed.enter();
                failed.record("ok", false);
            }
            // Outside any utterance: must not be recorded at all.
            let _orphan = tracing::info_span!("housekeeping").entered();
        });

        let rows = db.lock().unwrap().recent(20).unwrap();
        assert_eq!(rows.len(), 3, "orphan span leaked into the trace: {rows:?}");
        assert!(rows.iter().all(|r| r.utterance_id == 7));

        let stages: Vec<&str> = rows.iter().map(|r| r.stage.as_str()).collect();
        assert!(stages.contains(&"transcribe") && stages.contains(&"utterance"));

        let paste = rows.iter().find(|r| r.stage == "paste").unwrap();
        assert!(!paste.success);
        assert!(
            rows.iter()
                .find(|r| r.stage == "transcribe")
                .unwrap()
                .success
        );
        assert!(rows
            .iter()
            .find(|r| r.stage == "transcribe")
            .unwrap()
            .fields
            .contains("groq"));
    }
}
