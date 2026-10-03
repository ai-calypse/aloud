//! The socket itself: where it lives, and one request/response over it.
//!
//! Line-delimited JSON in both directions — an event up, a reply back, one
//! exchange per connection. The framing is a newline because both ends are
//! writing a single object and a length prefix would buy nothing.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

use crate::{Event, Reply};

/// How long the sidecar waits to reach the app.
///
/// This is the number that keeps a dead Aloud from taxing every tool call in
/// every terminal on the machine: if the app is not there, the hook gives up in
/// 200ms and the agent proceeds. Long enough for a busy but live app to accept.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);

/// How long to wait for a decision on a blocking request.
///
/// 24 hours, because the agent is parked on this process's stdout and the whole
/// point is to hold the decision point open until a human answers. Matches the
/// `timeout` the installer writes into the hook entry.
pub const PERMISSION_TIMEOUT: Duration = Duration::from_secs(86_400);

/// How long to wait on everything else. A status event nobody answers must not
/// hold a process open.
pub const EVENT_TIMEOUT: Duration = Duration::from_secs(45);

/// The hard limit on a Unix socket path, from `sun_path` in `sockaddr_un`.
///
/// 104 on macOS, 108 on Linux; the smaller is used so the check means the same
/// thing everywhere. Exceeding it fails at `bind` with `InvalidInput`, and the
/// visible symptom is the bridge never accepting a connection at all.
pub const MAX_SOCKET_PATH: usize = 104;

/// Where the socket lives.
///
/// A stable path, not keyed by pid: a hook process has no way to discover the
/// app's pid, and a path it cannot guess is a path it cannot connect to.
///
/// `~/Library/Application Support/Aloud/bridge.sock` is 64 bytes for a typical
/// home directory, leaving room under [`MAX_SOCKET_PATH`] — but a long enough
/// username would exceed it, so the length is checked rather than assumed.
pub fn socket_path() -> anyhow::Result<PathBuf> {
    let path = aloud_data_dir()?.join("bridge.sock");
    let length = path.as_os_str().len();
    anyhow::ensure!(
        length <= MAX_SOCKET_PATH,
        "socket path is {length} bytes, over the {MAX_SOCKET_PATH}-byte limit: {}",
        path.display()
    );
    Ok(path)
}

/// `~/Library/Application Support/Aloud` on macOS.
///
/// Duplicated from `aloud_notes::data_dir` rather than depended on: this crate
/// is linked into a binary that is spawned on every tool call of every agent,
/// and it has no business pulling in the notes crate to learn one path.
fn aloud_data_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    if cfg!(target_os = "macos") {
        Ok(home.join("Library/Application Support/Aloud"))
    } else {
        Ok(home.join(".local/share/Aloud"))
    }
}

/// Sends one event and waits for the app's reply.
///
/// Every failure is the same failure: `Ok(None)`, meaning "no directive". A
/// missing socket, a app that quit mid-request, a malformed reply and a closed
/// connection all have to look identical to the caller, because the agent's
/// behaviour in all four cases must be to carry on as though nothing were
/// installed.
#[cfg(unix)]
pub fn request(event: &Event, timeout: Duration) -> Option<Reply> {
    let path = socket_path().ok()?;
    // `connect_timeout` needs a resolved address, which Unix sockets do not
    // have; the connect itself is effectively immediate for a listening socket,
    // and the read timeout below is what bounds the wait that matters.
    let stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT)).ok()?;

    let mut line = serde_json::to_string(event).ok()?;
    line.push('\n');
    let mut writer = &stream;
    writer.write_all(line.as_bytes()).ok()?;
    writer.flush().ok()?;

    let mut reply = String::new();
    // EOF (the app quit with this request pending) reads as 0 bytes, which
    // takes the same path as never having connected.
    if BufReader::new(&stream).read_line(&mut reply).ok()? == 0 {
        return None;
    }
    serde_json::from_str(reply.trim()).ok()
}

#[cfg(not(unix))]
pub fn request(_event: &Event, _timeout: Duration) -> Option<Reply> {
    // ponytail: Windows would want a named pipe. Every agent path on Windows is
    // unexercised here, and failing open is the correct unimplemented state.
    None
}

/// The timeout an event deserves: the day-long block for `PermissionRequest`,
/// where the agent is parked on the hook's stdout until a human answers, and
/// 45 s for everything else — a status event nobody answers must not hold a
/// process open.
pub fn timeout_for(event: &Event) -> Duration {
    if event.is_permission_request() {
        PERMISSION_TIMEOUT
    } else {
        EVENT_TIMEOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_permission_request_gets_the_long_block() {
        let permission = Event {
            hook_event_name: Some("PermissionRequest".into()),
            ..Event::default()
        };
        assert_eq!(timeout_for(&permission), PERMISSION_TIMEOUT);

        // A PreToolUse never blocks — even on a write tool. Gating there was
        // tried and reverted; see AGENT_NOTCH.md's diagnosis.
        let pre = Event {
            hook_event_name: Some("PreToolUse".into()),
            tool_name: Some("Bash".into()),
            ..Event::default()
        };
        assert_eq!(timeout_for(&pre), EVENT_TIMEOUT);
        // An event with no name at all must not block for a day.
        assert_eq!(timeout_for(&Event::default()), EVENT_TIMEOUT);
    }

    #[test]
    fn the_socket_path_is_stable_and_not_keyed_by_pid() {
        // Reads HOME twice; a test swapping it in between failed this at random.
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(path) = socket_path() else { return };
        assert!(path.ends_with("bridge.sock"));
        assert_eq!(path, socket_path().unwrap(), "the path must not vary");
    }

    /// The app not running is the common case, not an error case: a hook fires
    /// on every tool call whether Aloud is open or not.
    #[cfg(unix)]
    #[test]
    fn a_request_with_nothing_listening_returns_no_directive() {
        // Points at a socket that does not exist by pointing HOME somewhere empty.
        let dir = short_tempdir();
        temp_env_home(dir.path(), || {
            assert_eq!(request(&Event::default(), Duration::from_millis(50)), None);
        });
    }

    /// A full round trip over a real socket, including the reply coming back.
    #[cfg(unix)]
    #[test]
    fn an_event_reaches_a_listener_and_its_reply_comes_back() {
        use std::os::unix::net::UnixListener;

        let dir = short_tempdir();
        temp_env_home(dir.path(), || {
            let path = socket_path().unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let listener = UnixListener::bind(&path).unwrap();

            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let got: Event = serde_json::from_str(line.trim()).unwrap();
                assert_eq!(got.tool_name.as_deref(), Some("Bash"));

                let mut reply = serde_json::to_string(&Reply::deny("nope")).unwrap();
                reply.push('\n');
                (&stream).write_all(reply.as_bytes()).unwrap();
            });

            let event = Event {
                tool_name: Some("Bash".into()),
                ..Event::default()
            };
            let reply = request(&event, Duration::from_secs(5)).expect("a reply");
            assert_eq!(reply, Reply::deny("nope"));
            server.join().unwrap();
        });
    }

    /// Quitting the app with a request pending closes the socket, which the
    /// hook sees as EOF. That must fail open like everything else — the
    /// alternative is an agent hung forever on a dead app's stdout.
    #[cfg(unix)]
    #[test]
    fn a_listener_that_hangs_up_without_answering_fails_open() {
        use std::os::unix::net::UnixListener;

        let dir = short_tempdir();
        temp_env_home(dir.path(), || {
            let path = socket_path().unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let listener = UnixListener::bind(&path).unwrap();

            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                // Drop without replying: exactly what quitting mid-request does.
                drop(stream);
            });

            assert_eq!(request(&Event::default(), Duration::from_secs(5)), None);
            server.join().unwrap();
        });
    }

    /// A tempdir short enough that the socket path underneath it still fits in
    /// `sun_path`. The default `/var/folders/...` prefix on macOS plus
    /// `Library/Application Support/Aloud/bridge.sock` does not.
    fn short_tempdir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("ab")
            .tempdir_in("/tmp")
            .unwrap()
    }

    /// The failure this catches is silent: over the limit, `bind` returns
    /// `InvalidInput` and the bridge simply never accepts a connection.
    #[test]
    fn an_over_long_socket_path_is_refused_with_a_reason() {
        let deep = std::path::PathBuf::from("/tmp").join("x".repeat(MAX_SOCKET_PATH));
        temp_env_home(&deep, || {
            let error = socket_path().expect_err("a path this long cannot bind");
            assert!(error.to_string().contains("over the"), "{error}");
        });
    }

    /// Held by every test that reads or swaps `HOME`.
    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `HOME` is process-wide, so these tests must not run beside each other.
    /// A mutex is cheaper than threading a base path through the real API for
    /// the sake of the tests.
    fn temp_env_home(home: &std::path::Path, body: impl FnOnce()) {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let previous = std::env::var_os("HOME");
        // SAFETY: single-threaded within the lock, and restored below.
        unsafe { std::env::set_var("HOME", home) };
        body();
        match previous {
            Some(value) => unsafe { std::env::set_var("HOME", value) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}
