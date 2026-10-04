//! Keep awake: `caffeinate -d` as a child process. Killing the child is the
//! off switch, so there is no assertion of ours to leak.

use std::process::{Child, Command, Stdio};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AwakeState {
    pub active: bool,
    /// When it lets go, in unix ms; None while active means "until stopped".
    #[specta(type = Option<specta_typescript::Number>)]
    pub until: Option<i64>,
    /// The length it was started with, so the segmented control can show which
    /// choice is live after the panel was closed and reopened.
    pub minutes: Option<u32>,
}

#[derive(Debug, Default)]
pub struct Awake {
    child: Option<Child>,
    until: Option<i64>,
    minutes: Option<u32>,
}

/// `-w <pid>` is the part that matters when Aloud is killed or crashes: the
/// child exits with it instead of keeping the Mac awake forever.
fn args(secs: Option<u32>, owner: u32) -> Vec<String> {
    let mut args = vec!["-d".to_string()];
    if let Some(secs) = secs {
        args.extend(["-t".into(), secs.to_string()]);
    }
    args.extend(["-w".into(), owner.to_string()]);
    args
}

impl Awake {
    /// Starts (or restarts, with the new length) for `minutes`, or until stopped.
    pub fn start(&mut self, minutes: Option<u32>, now_ms: i64) -> std::io::Result<()> {
        self.start_secs(minutes.map(|m| m.saturating_mul(60)), now_ms)?;
        self.minutes = minutes;
        Ok(())
    }

    fn start_secs(&mut self, secs: Option<u32>, now_ms: i64) -> std::io::Result<()> {
        self.stop();
        let child = Command::new("caffeinate")
            .args(args(secs, std::process::id()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        self.until = secs.map(|s| now_ms + i64::from(s) * 1000);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            // Reaped, or it lingers as a zombie until Aloud quits.
            let _ = child.wait();
        }
        self.until = None;
        self.minutes = None;
    }

    /// Whether it is still holding the Mac awake. A `caffeinate` that ran out
    /// by itself, or was killed from Activity Monitor, reads as inactive.
    pub fn state(&mut self) -> AwakeState {
        let alive = match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            Some(_) => false,
            None => false,
        };
        if !alive {
            self.stop();
        }
        AwakeState {
            active: alive,
            until: self.until,
            minutes: self.minutes,
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_dies_with_aloud_and_times_out_on_its_own() {
        assert_eq!(args(None, 7), ["-d", "-w", "7"]);
        assert_eq!(args(Some(1800), 7), ["-d", "-t", "1800", "-w", "7"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn stopping_ends_the_child_and_a_timeout_reads_as_inactive() {
        let mut awake = Awake::default();
        awake.start(Some(10), 1_000).unwrap();
        let s = awake.state();
        assert_eq!(
            (s.active, s.until, s.minutes),
            (true, Some(601_000), Some(10))
        );
        awake.stop();
        assert_eq!(awake.state(), AwakeState::default());

        // It leaves by itself: no stale "awake" after caffeinate's own timer.
        awake.start_secs(Some(1), 0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(!awake.state().active);
    }
}
