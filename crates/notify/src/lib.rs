//! Other apps' notifications in the notch: Messages, Mail, Slack, a ringing
//! FaceTime call, anything the user allows.
//!
//! The only source macOS gives a third party is Notification Center's own
//! SQLite database (`db`, see [`db`]), which `usernoted` writes and anyone with
//! Full Disk Access may read. [`watch`] polls it read-only for rows newer than
//! the newest one it saw at start, and hands each to the app, which decides
//! whether the user allowed that app and what the notch shows.
//!
//! Privacy is the shape of this crate: a [`Notification`]'s text lives in
//! memory from its row to the notch and is never logged (its `Debug` prints the
//! app and row id only), never stored and never sent anywhere. The database is
//! never written.
//!
//! ponytail: the schema is Apple's and undocumented. It is checked on open and
//! anything unrecognised is [`State::Unsupported`] rather than a guess; a macOS
//! that renames a column ends the mirror, never the app. The upgrade path, if
//! that ever matters, is a signed helper with the user-notifications
//! entitlement — which Apple grants to no third party for other apps' alerts.

pub mod db;

use std::fmt;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use aloud_islands::{
    Island, IslandAction, IslandKind, IslandPriority, IslandSource, IslandTier,
};
use serde::{Deserialize, Serialize};

pub use db::Notification;

/// Where the watcher stands, for Settings to say in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum State {
    /// Off, or not started yet.
    Off,
    Watching,
    /// The database is there and macOS will not let this app read it: Full
    /// Disk Access is the one fix.
    NeedsAccess,
    /// Readable but not what this build knows how to read.
    Unsupported { detail: String },
}

/// The apps mirrored once the user turns the mirror on, until they choose.
pub const DEFAULT_ALLOW: [&str; 5] = [
    "com.apple.MobileSMS",
    "com.apple.mail",
    "com.apple.FaceTime",
    "com.tinyspeck.slackmacgap",
    "com.apple.iCal",
];

/// How a notification is drawn, by who sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A ringing call: a card, urgent, until it is dealt with.
    Call,
    /// A message from a person, in Messages or a chat app.
    Message,
    /// Another app's banner: Mail, Calendar, anything allowed.
    Generic,
    /// Someone is sending a file.
    AirDrop,
}

/// An app's display name for the bundles whose names every Mac shares; None
/// for the rest, which the app resolves from the installed apps.
pub fn known_name(bundle: &str) -> Option<&'static str> {
    Some(match bundle {
        "com.apple.MobileSMS" => "Messages",
        "com.apple.mail" => "Mail",
        "com.apple.FaceTime" | "com.apple.TelephonyUtilities" => "FaceTime",
        "com.tinyspeck.slackmacgap" => "Slack",
        "com.apple.iCal" => "Calendar",
        "com.apple.reminders" => "Reminders",
        "com.apple.sharingd" => "AirDrop",
        _ => return None,
    })
}

pub fn kind(bundle: &str, n: &Notification) -> Kind {
    match bundle {
        // A FaceTime banner is a ring until it says otherwise. The words are
        // English-only, so another language's "missed" stays a card until
        // dismissed; the × is always there.
        "com.apple.FaceTime" | "com.apple.TelephonyUtilities" if !n.mentions("missed") => Kind::Call,
        "com.apple.MobileSMS" | "com.tinyspeck.slackmacgap" => Kind::Message,
        "com.apple.sharingd" => Kind::AirDrop,
        _ => Kind::Generic,
    }
}

/// The takeover for one notification. `app_name` and `icon` come from the
/// installed apps, which this crate does not scan.
///
/// Text is cut: a banner's body can be a whole email, and the line shows one.
pub fn island(n: &Notification, app_name: &str, icon: Option<String>) -> Island {
    let kind = kind(&n.app, n);
    let (detail, line) = match (n.app.as_str(), n.subtitle.is_empty(), n.body.is_empty()) {
        // Slack puts the channel in the subtitle: the design hangs it on the app name.
        ("com.tinyspeck.slackmacgap", false, false) => {
            (format!("{app_name} · {}", n.subtitle), n.body.clone())
        }
        _ => (
            app_name.to_string(),
            [n.subtitle.as_str(), n.body.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
    };
    let title = if n.title.is_empty() { app_name } else { &n.title };
    let (tier, priority) = match kind {
        Kind::Call => (IslandTier::Card, IslandPriority::Urgent),
        Kind::AirDrop => (IslandTier::Peek, IslandPriority::High),
        Kind::Message | Kind::Generic => (IslandTier::Peek, IslandPriority::Normal),
    };
    Island {
        id: format!("nudge:{}:{}", n.app, n.rec_id),
        kind: IslandKind::Alert,
        priority,
        icon,
        title: cut(title, 80),
        detail: Some(cut(&detail, 80)),
        body: (!line.is_empty()).then(|| cut(&line, 240)),
        // What a typing user sees: the sender, as a capsule.
        short: Some(cut(title, 16)),
        actions: vec![IslandAction::new("Open", "nudge.open").arg("bundle", &n.app)],
        tier: Some(tier),
        source: IslandSource::Banners,
        ..Island::default()
    }
}

fn cut(s: &str, max: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{}…", head.trim_end())
    } else {
        head
    }
}

/// How often the database is asked for its newest row.
///
/// ponytail: a one-second poll, not a kqueue on `db-wal`. The question is one
/// indexed `max(rec_id)` on an open read-only connection, microseconds and no
/// disk read when nothing changed; a file watcher would add a dependency to
/// learn about a WAL that checkpoints and rotates under it, for sub-second
/// latency a notch peek does not need. A watch on `db-wal` is the upgrade.
const POLL: Duration = Duration::from_secs(1);

/// Consecutive failed polls before the watcher gives up and says why.
const GIVE_UP: u32 = 5;

/// A running watcher; dropping it stops the thread within a poll.
pub struct Watch {
    _stop: mpsc::Sender<()>,
}

/// Starts watching on its own thread. `on_state` hears each state change (and
/// is the only word a refused or unrecognised database gets: one line in the
/// log, then the thread ends — no retry loop); `on_new` gets each new
/// notification, newest last.
pub fn watch(
    on_state: impl Fn(&State) + Send + 'static,
    on_new: impl Fn(Notification) + Send + 'static,
) -> Watch {
    let (stop, stopped) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("nudges".into())
        .spawn(move || run(&stopped, &on_state, &on_new));
    if let Err(e) = spawned {
        tracing::warn!(%e, "could not start the notification watcher");
    }
    Watch { _stop: stop }
}

fn run(stopped: &mpsc::Receiver<()>, on_state: &dyn Fn(&State), on_new: &dyn Fn(Notification)) {
    let mut reader = match db::Reader::open() {
        Ok(reader) => reader,
        Err(state) => {
            tracing::info!(?state, "notification mirror not started");
            on_state(&state);
            return;
        }
    };
    tracing::info!("notification mirror watching");
    on_state(&State::Watching);
    let mut failures = 0;
    loop {
        // The wait is also the stop signal: the sender dropping ends it.
        if !matches!(stopped.recv_timeout(POLL), Err(RecvTimeoutError::Timeout)) {
            return;
        }
        match reader.poll() {
            Ok(fresh) => {
                failures = 0;
                if !fresh.is_empty() {
                    // Counts only: the text is not ours to keep.
                    tracing::debug!(count = fresh.len(), "new notifications");
                }
                fresh.into_iter().for_each(on_new);
            }
            Err(state) => {
                failures += 1;
                if failures >= GIVE_UP {
                    tracing::info!(?state, "notification mirror stopped");
                    on_state(&state);
                    return;
                }
            }
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Off => f.write_str("off"),
            State::Watching => f.write_str("watching"),
            State::NeedsAccess => f.write_str("needs Full Disk Access"),
            State::Unsupported { detail } => write!(f, "unsupported: {detail}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(app: &str, title: &str, subtitle: &str, body: &str) -> Notification {
        Notification {
            rec_id: 7,
            app: app.into(),
            title: title.into(),
            subtitle: subtitle.into(),
            body: body.into(),
        }
    }

    #[test]
    fn a_ringing_facetime_banner_is_an_urgent_card_with_only_an_honest_button() {
        let island = island(&note("com.apple.FaceTime", "Maya Chen", "", "FaceTime Video"), "FaceTime", None);
        assert_eq!(island.tier, Some(IslandTier::Card));
        assert_eq!(island.priority, IslandPriority::Urgent);
        assert_eq!(island.id, "nudge:com.apple.FaceTime:7");
        let labels: Vec<_> = island.actions.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["Open"], "no Accept or Decline: Aloud cannot answer");
    }

    #[test]
    fn a_missed_call_is_news_not_a_sticky_card() {
        let island = island(&note("com.apple.FaceTime", "Maya Chen", "", "Missed call"), "FaceTime", None);
        assert_eq!(island.tier, Some(IslandTier::Peek));
        assert_eq!(island.priority, IslandPriority::Normal);
    }

    #[test]
    fn slack_hangs_the_channel_on_the_app_name_and_the_message_is_the_line() {
        let island = island(
            &note("com.tinyspeck.slackmacgap", "Priya", "#design", "Mockups are in the doc"),
            "Slack",
            None,
        );
        assert_eq!(island.detail.as_deref(), Some("Slack · #design"));
        assert_eq!(island.body.as_deref(), Some("Mockups are in the doc"));
    }

    #[test]
    fn mail_keeps_the_subject_and_the_preview_on_one_line() {
        let island = island(&note("com.apple.mail", "Linear", "Weekly digest", "12 issues closed"), "Mail", None);
        assert_eq!(island.title, "Linear");
        assert_eq!(island.body.as_deref(), Some("Weekly digest · 12 issues closed"));
    }

    #[test]
    fn airdrop_outranks_a_message_and_a_long_body_is_cut() {
        let airdrop = island(&note("com.apple.sharingd", "Maya", "", "IMG_2041.heic"), "AirDrop", None);
        assert_eq!(airdrop.priority, IslandPriority::High);
        let long = island(&note("com.apple.mail", "A", "", &"x".repeat(1000)), "Mail", None);
        assert!(long.body.unwrap().chars().count() <= 241);
    }

    #[test]
    fn a_notification_never_prints_its_text() {
        let n = note("com.apple.MobileSMS", "Maya", "", "my password is hunter2");
        let shown = format!("{n:?}");
        assert!(!shown.contains("hunter2") && !shown.contains("Maya"), "{shown}");
        assert!(shown.contains("com.apple.MobileSMS"));
    }
}
