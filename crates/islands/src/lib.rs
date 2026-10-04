//! The notch's live activities — "islands": an agent working, a write that
//! just landed, and later a timer, a download, a track. Any crate builds an
//! [`Island`]; the app pushes it through a [`Queue`], which decides what is
//! shown, in what order and for how long, and says what changed so the
//! webview follows along without re-reading the list.
//!
//! No Tauri here. The app (`src-tauri/src/islands.rs`) holds one queue, emits
//! its [`IslandChange`]s and runs the expiry clock; everything that can be wrong
//! quietly — ranking, dedupe, expiry — is in this file, under test.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How an island is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IslandKind {
    /// Ongoing: drawn beside the camera in the closed notch, and as a row in
    /// Now › Overview while it lasts.
    Live,
    /// Just happened: the chip that drops out of the notch, then goes.
    Alert,
}

/// What it costs to miss. Declared in rank order, so `Ord` is the ranking.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum IslandPriority {
    /// Ambient: worth a glance if there is room.
    Low,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

#[derive(Debug)]
struct Entry {
    island: Island,
    /// When it first arrived, as a counter: the tiebreak between equals.
    arrived: u64,
    deadline: Option<Instant>,
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
            entry.island = island.clone();
            return Some(IslandChange::Update(island));
        }
        self.arrivals += 1;
        self.entries.push(Entry {
            island: island.clone(),
            arrived: self.arrivals,
            deadline,
        });
        Some(IslandChange::Add(island))
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
            kind: IslandKind::Live,
            priority,
            icon: None,
            title: id.into(),
            detail: None,
            actions: Vec::new(),
            expires: None,
            dismissible: false,
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
