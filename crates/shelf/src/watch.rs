//! Folder watching, on `notify` (FSEvents on macOS).
//!
//! FSEvents says that something happened, not that it is finished: a browser
//! renames `.crdownload` to its final name while the last bytes are still
//! landing, and macOS writes a screenshot a moment after announcing it. So a
//! path is only reported once its size has stopped moving — [`Settler`], with
//! the clock and the file system passed in so the rule is testable.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How often a settling path is looked at again.
const TICK: Duration = Duration::from_millis(250);

/// Keeps a watcher alive: dropping it stops the watch and its thread.
pub struct Handle {
    _watcher: Option<RecommendedWatcher>,
    stop: Arc<AtomicBool>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Change {
    path: PathBuf,
    /// A file appeared or was renamed into place — the only events that mean
    /// "new", as against one being edited, which a download never is.
    arrival: bool,
}

fn raw(dir: &Path) -> Result<(Handle, mpsc::Receiver<Change>)> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        let arrival = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
        );
        for path in event.paths {
            // A closed receiver means the watch is being dropped.
            let _ = tx.send(Change { path, arrival });
        }
    })?;
    watcher
        .watch(dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("could not watch {}", dir.display()))?;
    Ok((
        Handle {
            _watcher: Some(watcher),
            stop: Arc::default(),
        },
        rx,
    ))
}

/// Calls `on` once for each new path in `dir` that passes `accept` and has
/// stopped growing for `settle`.
pub fn settled(
    dir: &Path,
    accept: fn(&Path) -> bool,
    settle: Duration,
    mut on: impl FnMut(PathBuf) + Send + 'static,
) -> Result<Handle> {
    let (handle, rx) = raw(dir)?;
    std::thread::spawn(move || {
        let mut settler = Settler::new(settle);
        loop {
            match rx.recv_timeout(TICK) {
                Ok(change) => {
                    if change.arrival && accept(&change.path) {
                        settler.observe(change.path, Instant::now());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let ready = settler.tick(Instant::now(), |p| {
                std::fs::metadata(p).ok().map(|m| m.len())
            });
            for path in ready {
                on(path);
            }
        }
    });
    Ok(handle)
}

/// Calls `on` whenever the set of names in `dir` changes, checked every
/// `every`.
///
/// This is polling, not FSEvents, on purpose: watching `/Volumes` with
/// `notify` was tried on real disk images and missed mounts and unmounts
/// (they are mount events, which FSEvents reports unreliably for a directory
/// that is not itself on the volume). A `read_dir` of a handful of entries
/// every couple of seconds costs nothing and does not miss.
pub fn names_changed(dir: &Path, every: Duration, mut on: impl FnMut() + Send + 'static) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let dir = dir.to_path_buf();
    let flag = stop.clone();
    std::thread::spawn(move || {
        let names = || -> BTreeSet<std::ffi::OsString> {
            std::fs::read_dir(&dir)
                .map(|d| d.flatten().map(|e| e.file_name()).collect())
                .unwrap_or_default()
        };
        let mut last = names();
        while !flag.load(Ordering::Relaxed) {
            std::thread::sleep(every);
            let now = names();
            if now != last {
                last = now;
                on();
            }
        }
    });
    Handle {
        _watcher: None,
        stop,
    }
}

struct Pending {
    size: Option<u64>,
    since: Instant,
}

/// Decides when a path that just appeared is done being written.
pub struct Settler {
    settle: Duration,
    pending: HashMap<PathBuf, Pending>,
    /// When each path was last reported, so the burst of events a finished
    /// file also throws off does not announce it twice.
    reported: HashMap<PathBuf, Instant>,
}

/// A path reported this recently is not reported again.
const REPORT_QUIET: Duration = Duration::from_secs(5);

impl Settler {
    pub fn new(settle: Duration) -> Self {
        Self {
            settle,
            pending: HashMap::new(),
            reported: HashMap::new(),
        }
    }

    /// A path appeared or was renamed into place. Seeing it again restarts
    /// its wait: it was just written to.
    pub fn observe(&mut self, path: PathBuf, now: Instant) {
        if self
            .reported
            .get(&path)
            .is_some_and(|at| now.duration_since(*at) < REPORT_QUIET)
        {
            return;
        }
        self.pending.insert(
            path,
            Pending {
                size: None,
                since: now,
            },
        );
    }

    /// The paths whose size held still for `settle`. `stat` is the file's
    /// size, or None when it is gone — the old name of a rename, or a
    /// temporary file the browser deleted.
    pub fn tick(&mut self, now: Instant, stat: impl Fn(&Path) -> Option<u64>) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        self.pending.retain(|path, p| match stat(path) {
            None => false,
            Some(size) if p.size == Some(size) && now.duration_since(p.since) >= self.settle => {
                ready.push(path.clone());
                false
            }
            Some(size) => {
                if p.size != Some(size) {
                    p.size = Some(size);
                    p.since = now;
                }
                true
            }
        });
        for path in &ready {
            self.reported.insert(path.clone(), now);
        }
        self.reported
            .retain(|_, at| now.duration_since(*at) < REPORT_QUIET);
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const SETTLE: Duration = Duration::from_millis(500);

    fn secs(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_file_still_growing_is_not_reported_until_it_stops() {
        let t0 = Instant::now();
        let size = Cell::new(10u64);
        let mut s = Settler::new(SETTLE);
        s.observe("/d/a.zip".into(), t0);

        assert!(s.tick(t0 + secs(250), |_| Some(size.get())).is_empty());
        size.set(20); // more bytes arrived
        assert!(s.tick(t0 + secs(750), |_| Some(size.get())).is_empty());
        // Size held from 750 on: not yet 500ms of stillness at 1000…
        assert!(s.tick(t0 + secs(1000), |_| Some(size.get())).is_empty());
        // …and enough at 1250.
        assert_eq!(
            s.tick(t0 + secs(1250), |_| Some(size.get())),
            vec![PathBuf::from("/d/a.zip")]
        );
    }

    #[test]
    fn the_old_name_of_a_rename_is_dropped_not_reported() {
        let t0 = Instant::now();
        let mut s = Settler::new(SETTLE);
        s.observe("/d/a.crdownload".into(), t0);
        assert!(s.tick(t0 + secs(250), |_| None).is_empty());
        // Gone for good: nothing left waiting.
        assert!(s.tick(t0 + secs(5000), |_| Some(1)).is_empty());
    }

    #[test]
    fn the_events_a_finished_file_throws_off_do_not_report_it_twice() {
        let t0 = Instant::now();
        let mut s = Settler::new(SETTLE);
        s.observe("/d/a.zip".into(), t0);
        s.tick(t0 + secs(250), |_| Some(5));
        assert_eq!(s.tick(t0 + secs(750), |_| Some(5)).len(), 1);

        s.observe("/d/a.zip".into(), t0 + secs(800));
        assert!(s.tick(t0 + secs(2000), |_| Some(5)).is_empty());

        // The same name later is a new download.
        s.tick(t0 + secs(7000), |_| Some(5)); // expires the memory
        s.observe("/d/a.zip".into(), t0 + secs(7100));
        s.tick(t0 + secs(7200), |_| Some(5));
        assert_eq!(s.tick(t0 + secs(7800), |_| Some(5)).len(), 1);
    }
}
