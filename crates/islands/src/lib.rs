//! The notch's live activities — "islands": an agent working, a write that
//! just landed, and later a timer, a download, a track. Any crate builds an
//! [`Island`]; the app pushes it through a [`Queue`], which decides what is
//! shown, in what order and for how long, and says what changed so the
//! webview follows along without re-reading the list.
//!
//! No Tauri here. The app (`src-tauri/src/islands.rs`) holds one queue, emits
//! its [`IslandChange`]s and runs the expiry clock; everything that can be wrong
//! quietly — ranking, dedupe, expiry — is in this file, under test.
//!
//! An alert is a *takeover*: it borrows the notch for a moment, sized by what
//! it costs to miss ([`IslandTier`]). Whether it may show now, wait or be dropped is
//! [`gate`]'s call; [`stage`] keeps the one on screen and the line behind it.

pub mod gate;
pub mod stage;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How an island is drawn.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IslandKind {
    /// Ongoing: drawn beside the camera in the closed notch, and as a row in
    /// Now › Overview while it lasts.
    #[default]
    Live,
    /// Just happened: a takeover, drawn at its [`IslandTier`]'s size, then gone.
    Alert,
}

/// How much of the notch a takeover takes: size follows the cost of missing it.
/// Declared smallest first, so `Ord` is the size.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IslandTier {
    /// A side extension at the notch's own height: a glyph and one word or
    /// number, 1.2–2 s, no buttons.
    Whisper,
    /// One extra row: app, title, one line, 4 s, opens on hover.
    Peek,
    /// The big drop, sticky, with buttons. Only for something that needs the
    /// user: a call, an approval, battery under 10%, a failed critical action.
    Card,
}

impl IslandTier {
    /// What Quiet mode shows it as: one size smaller. A whisper has nowhere
    /// smaller to go.
    pub const fn quieter(self) -> Self {
        match self {
            IslandTier::Card => IslandTier::Peek,
            IslandTier::Peek | IslandTier::Whisper => IslandTier::Whisper,
        }
    }
}

/// Who raised a takeover, which decides the rules that apply to it and the
/// Settings switch that can silence it.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IslandSource {
    /// Aloud's own: a confirmation, a timer, a failed action. Never held back
    /// by Focus and never switched off here.
    #[default]
    Own,
    /// The user pressed a key for it (volume, mic mute, Caps Lock): it draws
    /// at once, even over a whisper or a card, because it was asked for.
    Hud,
    /// Battery, charger, Low Power Mode.
    Power,
    /// Drives, displays, Wi-Fi, VPN, Bluetooth.
    Connectivity,
    /// Something that happened in another app: a screenshot, a download.
    Banners,
}

impl IslandSource {
    /// Raised by macOS or another app rather than by Aloud: what Focus holds back.
    pub const fn is_system(self) -> bool {
        !matches!(self, IslandSource::Own)
    }

    pub const fn is_direct(self) -> bool {
        matches!(self, IslandSource::Hud)
    }
}

/// What it costs to miss. Declared in rank order, so `Ord` is the ranking.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IslandPriority {
    /// Ambient: worth a glance if there is room.
    Low,
    #[default]
    Normal,
    /// Time-bound: a timer about to end, a download finishing.
    High,
    /// Waiting on the user: an agent asking, a call ringing.
    Urgent,
}

/// A button on an island.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IslandAction {
    /// The button's word: "Show", "Undo", "Review".
    pub label: String,
    /// An id in the webview's action registry (`features/notch/actions.ts`),
    /// the one ⌘K runs too — so an island never carries code, only a name.
    pub action: String,
    /// Its arguments. Strings, because specta will not export a JSON value.
    pub args: BTreeMap<String, String>,
}

impl IslandAction {
    pub fn new(label: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: action.into(),
            args: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn arg(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.args.insert(key.into(), value.into());
        self
    }
}

/// One live activity.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Island {
    /// Stable for the activity's life: pushing the same id again updates it
    /// in place. Namespace by owner — `agents`, `toast:3`, `download:<path>`.
    pub id: String,
    pub kind: IslandKind,
    pub priority: IslandPriority,
    /// A `data:` image (an app's icon, as `aloud_selection::app_icon` and
    /// `aloud_agent::icons` make them), or the name of a glyph the webview
    /// knows (`agents`, `download`, `timer`…). None draws a neutral glyph.
    pub icon: Option<String>,
    /// What it is: "Claude · aloud-tauri", "Sent the email to David".
    pub title: String,
    /// Its live value, short enough for the closed notch: "2 running",
    /// "18:42". For an alert, the app it happened in.
    pub detail: Option<String>,
    pub actions: Vec<IslandAction>,
    /// Milliseconds it lives after its last push; None lives until removed.
    /// `u32` and not a duration type: specta refuses 64-bit integers, which
    /// would reach the webview as a BigInt.
    pub expires: Option<u32>,
    /// Draws a × in Now › Overview that takes it down (`dismiss_island`). Only
    /// for an island whose owner remembers the dismissal, or its next poll
    /// puts it straight back.
    #[serde(default)]
    pub dismissible: bool,
    /// The size of the takeover. None is what every push made before tiers
    /// means: an alert is a [`IslandTier::Peek`], and a live island is no takeover
    /// at all — see [`Island::takeover_tier`].
    #[serde(default)]
    #[specta(optional)]
    pub tier: Option<IslandTier>,
    /// A level for a bar, 0–100: volume, brightness. Updates to it alone are
    /// rate-limited by [`Queue::push`].
    #[serde(default)]
    #[specta(optional)]
    pub level: Option<u8>,
    /// How long it stays, in milliseconds, before it leaves on its own. None
    /// is its tier's default ([`gate::dwell_ms`]); a card has none.
    #[serde(default)]
    #[specta(optional)]
    pub dwell_ms: Option<u32>,
    /// A whisper's one word or number — "Sent", "20%" — which `detail`
    /// (the app, for an alert) cannot be.
    #[serde(default)]
    #[specta(optional)]
    pub short: Option<String>,
    #[serde(default)]
    pub source: IslandSource,
    /// Set by the gate, never by a producer: a peek or card shown as a side
    /// capsule because the user is typing, opened by pointing at it.
    #[serde(default)]
    pub shrunk: bool,
}

impl Island {
    /// The tier it takes over at; None for an island that is not a takeover.
    pub fn takeover_tier(&self) -> Option<IslandTier> {
        self.tier.or(match self.kind {
            IslandKind::Alert => Some(IslandTier::Peek),
            IslandKind::Live => None,
        })
    }
}

/// What a push, a removal or the clock did to the queue — the `island`
/// event's payload, applied by the webview in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "op", content = "value")]
pub enum IslandChange {
    Add(Island),
    Update(Island),
    /// By id.
    Remove(String),
}

/// The closest two level-only updates to one island may be sent. A held volume
/// key repeats about every 30 ms; the bar eases between values on its own, so
/// faster sends only cost the webview renders.
pub const LEVEL_GAP: Duration = Duration::from_millis(60);

#[derive(Debug)]
struct Entry {
    island: Island,
    /// When it first arrived, as a counter: the tiebreak between equals.
    arrived: u64,
    deadline: Option<Instant>,
    /// When the webview was last sent this island.
    sent: Instant,
    /// A newer level is stored but not yet sent: see [`Queue::release_held`].
    held: bool,
}

/// Every island on screen.
///
/// ponytail: a Vec scanned linearly. A notch holds a handful of activities;
/// a map earns its place past dozens, which the closed notch cannot draw.
#[derive(Debug, Default)]
pub struct Queue {
    entries: Vec<Entry>,
    arrivals: u64,
}

impl Queue {
    /// Const, so the app can keep one in a `static`.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            arrivals: 0,
        }
    }

    /// Adds `island`, or updates the one with its id in place — keeping its
    /// place among equals, so an update never reshuffles the notch.
    ///
    /// Returns None when nothing visible changed: a poller pushing the same
    /// state every few seconds must not wake the webview each time. A re-push
    /// still renews the island's lease, identical or not.
    pub fn push(&mut self, island: Island, now: Instant) -> Option<IslandChange> {
        let deadline = island
            .expires
            .map(|ms| now + Duration::from_millis(u64::from(ms)));
        if let Some(entry) = self.entries.iter_mut().find(|e| e.island.id == island.id) {
            entry.deadline = deadline;
            if entry.island == island {
                return None;
            }
            let level_only = Island {
                level: entry.island.level,
                ..island.clone()
            } == entry.island;
            let too_soon = now.saturating_duration_since(entry.sent) < LEVEL_GAP;
            entry.island = island.clone();
            if level_only && too_soon {
                entry.held = true;
                return None;
            }
            entry.sent = now;
            entry.held = false;
            return Some(IslandChange::Update(island));
        }
        self.arrivals += 1;
        self.entries.push(Entry {
            island: island.clone(),
            arrived: self.arrivals,
            deadline,
            sent: now,
            held: false,
        });
        Some(IslandChange::Add(island))
    }

    /// Sends the newest level of every island whose last send is old enough:
    /// the trailing edge of the rate limit, so a bar always lands on the value
    /// the user stopped at rather than one step short of it.
    pub fn release_held(&mut self, now: Instant) -> Vec<IslandChange> {
        let mut out = Vec::new();
        for entry in &mut self.entries {
            if entry.held && now.saturating_duration_since(entry.sent) >= LEVEL_GAP {
                entry.held = false;
                entry.sent = now;
                out.push(IslandChange::Update(entry.island.clone()));
            }
        }
        out
    }

    /// Whether a level is waiting for [`Queue::release_held`].
    pub fn has_held(&self) -> bool {
        self.entries.iter().any(|e| e.held)
    }

    /// Takes an island out — its owner says it is over, or the user
    /// dismissed it. None if it was already gone, which is not an error: the
    /// clock and the user can race to the same island.
    pub fn remove(&mut self, id: &str) -> Option<IslandChange> {
        let at = self.entries.iter().position(|e| e.island.id == id)?;
        self.entries.remove(at);
        Some(IslandChange::Remove(id.to_string()))
    }

    /// Takes out every island whose time is up at `now`.
    pub fn expire(&mut self, now: Instant) -> Vec<IslandChange> {
        let mut gone = Vec::new();
        self.entries.retain(|e| {
            let due = e.deadline.is_some_and(|deadline| deadline <= now);
            if due {
                gone.push(IslandChange::Remove(e.island.id.clone()));
            }
            !due
        });
        gone
    }

    /// When the next island falls due, for the app's clock.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries.iter().filter_map(|e| e.deadline).min()
    }

    /// The order to draw them in: what costs most to miss first, then the
    /// order they arrived — so a newcomer of equal weight joins the end
    /// rather than pushing aside what the user is already reading.
    pub fn ranked(&self) -> Vec<Island> {
        let mut entries: Vec<&Entry> = self.entries.iter().collect();
        entries.sort_by(|a, b| {
            b.island
                .priority
                .cmp(&a.island.priority)
                .then(a.arrived.cmp(&b.arrived))
        });
        entries.into_iter().map(|e| e.island.clone()).collect()
    }

    /// [`Queue::ranked`]'s ids: sent beside every change, so the webview
    /// orders its copy without a second ranking that could disagree.
    pub fn order(&self) -> Vec<String> {
        self.ranked().into_iter().map(|island| island.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn island(id: &str, priority: IslandPriority) -> Island {
        Island {
            id: id.into(),
            priority,
            title: id.into(),
            ..Island::default()
        }
    }

    fn ids(queue: &Queue) -> Vec<String> {
        queue.order()
    }

    #[test]
    fn ranks_by_what_it_costs_to_miss_then_by_arrival() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(island("music", IslandPriority::Low), now);
        queue.push(island("timer", IslandPriority::High), now);
        queue.push(island("download", IslandPriority::High), now);
        queue.push(island("agents", IslandPriority::Urgent), now);
        assert_eq!(ids(&queue), ["agents", "timer", "download", "music"]);
    }

    /// The failure: an update re-inserting at the end, so a timer ticking
    /// every second hopped past its equals and the notch reshuffled.
    #[test]
    fn an_update_keeps_its_place_among_equals() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(island("timer", IslandPriority::Normal), now);
        queue.push(island("download", IslandPriority::Normal), now);

        let mut ticked = island("timer", IslandPriority::Normal);
        ticked.detail = Some("18:41".into());
        assert_eq!(
            queue.push(ticked.clone(), now),
            Some(IslandChange::Update(ticked))
        );
        assert_eq!(ids(&queue), ["timer", "download"]);
    }

    #[test]
    fn a_raised_priority_moves_it_up() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(island("download", IslandPriority::Normal), now);
        queue.push(island("agents", IslandPriority::Normal), now);
        queue.push(island("agents", IslandPriority::Urgent), now);
        assert_eq!(ids(&queue), ["agents", "download"]);
    }

    /// Dedupe: a poller re-pushing the same state must not emit, or every
    /// three-second poll would restart the webview's animations.
    #[test]
    fn an_identical_push_is_not_a_change() {
        let now = Instant::now();
        let mut queue = Queue::new();
        assert!(matches!(
            queue.push(island("agents", IslandPriority::Normal), now),
            Some(IslandChange::Add(_))
        ));
        assert_eq!(
            queue.push(island("agents", IslandPriority::Normal), now),
            None
        );
        assert_eq!(queue.ranked().len(), 1);
    }

    #[test]
    fn expires_only_what_is_due() {
        let now = Instant::now();
        let mut queue = Queue::new();
        let mut toast = island("toast:1", IslandPriority::Normal);
        toast.expires = Some(4_000);
        queue.push(toast, now);
        queue.push(island("agents", IslandPriority::Normal), now);

        assert!(queue.expire(now + Duration::from_millis(3_999)).is_empty());
        assert_eq!(
            queue.expire(now + Duration::from_millis(4_000)),
            [IslandChange::Remove("toast:1".into())]
        );
        assert_eq!(ids(&queue), ["agents"], "one with no expiry lives on");
        assert_eq!(queue.next_deadline(), None);
    }

    /// A re-push renews the lease even when nothing visible changed — a live
    /// activity kept alive by its owner's poll must not time out under it.
    #[test]
    fn a_repush_renews_the_lease() {
        let now = Instant::now();
        let mut queue = Queue::new();
        let mut download = island("download", IslandPriority::Normal);
        download.expires = Some(1_000);
        queue.push(download.clone(), now);
        assert_eq!(queue.push(download, now + Duration::from_millis(900)), None);

        assert!(queue.expire(now + Duration::from_millis(1_500)).is_empty());
        assert_eq!(
            queue.next_deadline(),
            Some(now + Duration::from_millis(1_900))
        );
    }

    #[test]
    fn removing_twice_is_not_an_error() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(island("agents", IslandPriority::Normal), now);
        assert_eq!(
            queue.remove("agents"),
            Some(IslandChange::Remove("agents".into()))
        );
        assert_eq!(queue.remove("agents"), None);
    }

    fn level(id: &str, level: u8) -> Island {
        Island {
            level: Some(level),
            ..island(id, IslandPriority::Normal)
        }
    }

    /// The failure: a held volume key sending an `island` event per repeat,
    /// thirty renders a second for a bar that eases on its own.
    #[test]
    fn level_updates_are_rate_limited_and_land_on_the_last_value() {
        let now = Instant::now();
        let mut queue = Queue::new();
        assert!(matches!(
            queue.push(level("volume", 10), now),
            Some(IslandChange::Add(_))
        ));
        let at = |ms| now + Duration::from_millis(ms);
        assert_eq!(queue.push(level("volume", 12), at(20)), None);
        assert_eq!(queue.push(level("volume", 14), at(40)), None);
        assert!(queue.has_held());
        assert!(queue.release_held(at(50)).is_empty(), "not yet");

        assert_eq!(
            queue.release_held(at(60)),
            [IslandChange::Update(level("volume", 14))],
            "the trailing edge sends the newest, not the first held"
        );
        assert!(!queue.has_held());
    }

    #[test]
    fn a_level_update_after_the_gap_goes_straight_out() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(level("volume", 10), now);
        let later = now + LEVEL_GAP;
        assert_eq!(
            queue.push(level("volume", 11), later),
            Some(IslandChange::Update(level("volume", 11)))
        );
    }

    /// Only the level is rate-limited: a title change is news, however soon.
    #[test]
    fn a_change_beyond_the_level_is_not_held() {
        let now = Instant::now();
        let mut queue = Queue::new();
        queue.push(level("volume", 10), now);
        let mut muted = level("volume", 0);
        muted.title = "Muted".into();
        assert!(matches!(
            queue.push(muted, now + Duration::from_millis(5)),
            Some(IslandChange::Update(_))
        ));
    }

    /// The webview switches on `op` and reads `value`; a serde attribute
    /// changing this shape would break it without a compile error.
    #[test]
    fn a_change_crosses_the_wire_as_op_and_value() {
        let json = serde_json::to_value(IslandChange::Remove("agents".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "op": "remove", "value": "agents" })
        );

        let mut alert = island("toast:1", IslandPriority::Normal);
        alert.kind = IslandKind::Alert;
        alert.actions = vec![IslandAction::new("Show", "result.show").arg("app", "Notes")];
        let json = serde_json::to_value(IslandChange::Add(alert)).unwrap();
        assert_eq!(json["op"], "add");
        assert_eq!(json["value"]["kind"], "alert");
        assert_eq!(json["value"]["actions"][0]["args"]["app"], "Notes");
    }
}
