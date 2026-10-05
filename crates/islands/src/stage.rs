//! The notch's one stage: the takeover on it, the whisper riding it, and the
//! line of events waiting behind. [`crate::gate::decide`] says what an event may
//! do; this keeps the state that decision reads and applies it.
//!
//! No clock and no window: callers pass `now` and perform the [`Action`]s, so
//! a realistic sequence of events is a plain test (and `examples/`).

use std::time::{Duration, Instant};

use crate::gate::{self, Ctx, Current, Decision};
use crate::{Island, IslandSource, IslandTier};

/// How long past its dwell a takeover may stay if the webview never says it is
/// done. Pointing at one pauses the webview's clock, so this has to outlast a
/// glance; it is a backstop for a webview that never reports, not the dwell.
pub const HOVER_GRACE: Duration = Duration::from_secs(8);

/// A beat between one takeover leaving and the next arriving, so two do not
/// read as one thing changing.
pub const GAP: Duration = Duration::from_millis(150);

/// How long a whisper that rides a peek's top row, or echoes under a card, stays.
const SIDE_DWELL_MS: u32 = 1_500;

/// The caller's current view of the world, read fresh for every decision.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub panel_open: bool,
    pub typing: bool,
    pub focus_on: bool,
    pub quiet: bool,
    pub muted: Vec<IslandSource>,
}

/// What the caller has to do. Islands are returned ready to push: tier as it
/// is to be drawn, dwell resolved, `shrunk` set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Draw this as the takeover on screen, replacing any other.
    Show(Island),
    /// Draw this beside it: a whisper on a peek's top row, or an echo under a
    /// card. The takeover underneath is untouched.
    Side(Island),
    /// Take this island down.
    Clear(String),
    /// The number of events waiting changed: "+N".
    Waiting(usize),
}

#[derive(Debug, Clone)]
struct Shown {
    island: Island,
    /// As drawn: a shrunk peek reads as a whisper to the rules.
    tier: IslandTier,
    /// When it first arrived, which an interrupted one keeps: it goes back to
    /// the front of its priority, not the end.
    born: Instant,
    /// When it went up, which its dwell counts from.
    since: Instant,
    dwell_ms: Option<u32>,
}

impl Shown {
    fn current(&self) -> Current {
        Current {
            tier: self.tier,
            priority: self.island.priority,
            // ponytail: never expanded here. The webview opens a peek on
            // hover without telling Rust, so a whisper may overlay a peek the
            // user is pointing at; reporting hover is the upgrade.
            expanded: false,
        }
    }

    fn overdue(&self, now: Instant) -> bool {
        self.dwell_ms.is_some_and(|ms| {
            now.saturating_duration_since(self.since)
                > Duration::from_millis(u64::from(ms)) + HOVER_GRACE
        })
    }

    fn remaining_ms(&self, now: Instant) -> Option<u32> {
        let ms = self.dwell_ms?;
        let spent = now.saturating_duration_since(self.since).as_millis();
        // Never less than a glance: it resumes to be read, not to flash.
        Some((u128::from(ms).saturating_sub(spent) as u32).max(1_500))
    }
}

#[derive(Debug, Clone)]
struct Held {
    island: Island,
    born: Instant,
    /// Set when it was pushed aside mid-dwell: it resumes with what was left.
    resume_ms: Option<u32>,
    resumed: bool,
}

#[derive(Debug, Default)]
pub struct Stage {
    current: Option<Shown>,
    side: Option<Shown>,
    waiting: Vec<Held>,
    /// When the stage last emptied, for [`GAP`].
    idle_since: Option<Instant>,
    /// The "+N" last announced, so it is announced only when it changes.
    told: usize,
}

impl Stage {
    pub const fn new() -> Self {
        Self {
            current: None,
            side: None,
            waiting: Vec::new(),
            idle_since: None,
            told: 0,
        }
    }

    /// Whether anything needs the clock: something shown, or something waiting.
    pub fn is_active(&self) -> bool {
        self.current.is_some() || self.side.is_some() || !self.waiting.is_empty()
    }

    /// The id of the takeover on screen, if any.
    pub fn on_stage(&self) -> Option<&str> {
        self.current.as_ref().map(|s| s.island.id.as_str())
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// An event arrives. The decision comes back with the actions so the app
    /// can log it: every outcome is something the user may ask about.
    pub fn offer(&mut self, island: Island, env: &Env, now: Instant) -> (Decision, Vec<Action>) {
        let mut out = Vec::new();
        // The same id updates in place and restarts its clock: hold volume down
        // and it is one whisper, not thirty.
        if let Some(action) = self.coalesce(&island, env, now) {
            out.push(action);
            return (Decision::Show { shrunk: false }, out);
        }
        if let Some(held) = self.waiting.iter_mut().find(|h| h.island.id == island.id) {
            held.island = island;
            return (Decision::Queue(gate::Hold::Behind), out);
        }

        let decision = self.decide(&island, env);
        let held = Held {
            island,
            born: now,
            resume_ms: None,
            resumed: false,
        };
        match decision {
            Decision::Drop(_) => {}
            Decision::Queue(_) => self.waiting.push(held),
            Decision::Show { shrunk } => self.show(held, env, shrunk, now, &mut out),
            Decision::Interrupt { shrunk } => self.interrupt(held, env, shrunk, now, &mut out),
            Decision::Overlay | Decision::Echo => {
                let side = Self::side(held.island, now, now);
                out.push(Action::Side(side.island.clone()));
                let replaced = self.side.replace(side);
                out.extend(replaced.map(|r| Action::Clear(r.island.id)));
            }
        }
        self.announce(&mut out);
        (decision, out)
    }

    /// An island went away — dismissed, expired, taken down by its owner. The
    /// next waiting event takes the stage on a later [`Stage::tick`].
    pub fn end(&mut self, id: &str, now: Instant) {
        if self.current.as_ref().is_some_and(|s| s.island.id == id) {
            self.current = None;
            self.idle_since = Some(now);
        }
        if self.side.as_ref().is_some_and(|s| s.island.id == id) {
            self.side = None;
        }
        self.waiting.retain(|h| h.island.id != id);
    }

    /// The user opened a panel over the notch, which the takeover cannot
    /// outlive. A card has not been answered, so it waits for the panel to
    /// close; a peek or whisper was only news and is let go.
    pub fn panel_opened(&mut self, now: Instant) -> Vec<Action> {
        let mut out = Vec::new();
        if let Some(side) = self.side.take() {
            out.push(Action::Clear(side.island.id));
        }
        if let Some(shown) = self.current.take() {
            self.idle_since = Some(now);
            out.push(Action::Clear(shown.island.id.clone()));
            if shown.tier == IslandTier::Card {
                self.waiting.push(Held {
                    island: shown.island,
                    born: shown.born,
                    resume_ms: None,
                    resumed: true,
                });
            }
        }
        self.announce(&mut out);
        out
    }

    /// The clock: lets go of what overstayed, drops what went stale, and puts
    /// the next waiting event up if the stage is free.
    pub fn tick(&mut self, env: &Env, now: Instant) -> Vec<Action> {
        let mut out = Vec::new();
        if self.current.as_ref().is_some_and(|s| s.overdue(now)) {
            if let Some(shown) = self.current.take() {
                self.idle_since = Some(now);
                out.push(Action::Clear(shown.island.id));
            }
        }
        if self.side.as_ref().is_some_and(|s| s.overdue(now)) {
            if let Some(side) = self.side.take() {
                out.push(Action::Clear(side.island.id));
            }
        }
        let quiet = env.quiet;
        self.waiting.retain(|h| {
            h.resumed
                || gate::stale_after(gate::tier_of(&h.island, quiet), h.island.priority)
                    .is_none_or(|ttl| now.saturating_duration_since(h.born) <= ttl)
        });
        self.advance(env, now, &mut out);
        self.announce(&mut out);
        out
    }

    /// [`gate::decide`] with one house rule: the design lets an urgent event
    /// break into an open panel as a banner, and that banner lives in the
    /// panel's own UI. Until it exists the event waits for the panel to close,
    /// rather than showing on a window the panel owns.
    ///
    /// ponytail: urgent waits behind the user's panel. The upgrade is a
    /// `Decision::Banner` that `NotchShell` draws.
    fn decide(&self, island: &Island, env: &Env) -> Decision {
        match gate::decide(island, &self.ctx(env)) {
            Decision::Queue(why) => Decision::Queue(why),
            Decision::Drop(why) => Decision::Drop(why),
            _ if env.panel_open => Decision::Queue(gate::Hold::PanelOpen),
            decision => decision,
        }
    }

    fn decide_waiting(&self, at: usize, env: &Env) -> Decision {
        let mut ctx = self.ctx(env);
        ctx.waiting = self.waiting.len() > 1;
        match gate::decide(&self.waiting[at].island, &ctx) {
            _ if env.panel_open => Decision::Queue(gate::Hold::PanelOpen),
            decision => decision,
        }
    }

    fn ctx(&self, env: &Env) -> Ctx {
        Ctx {
            current: self.current.as_ref().map(Shown::current),
            waiting: !self.waiting.is_empty(),
            panel_open: env.panel_open,
            typing: env.typing,
            focus_on: env.focus_on,
            quiet: env.quiet,
            muted: env.muted.clone(),
        }
    }

    fn coalesce(&mut self, island: &Island, env: &Env, now: Instant) -> Option<Action> {
        if let Some(old) = self.current.as_ref().filter(|s| s.island.id == island.id) {
            let shown = Self::prepare(island.clone(), env, old.island.shrunk, None, old.born, now);
            let action = Action::Show(shown.island.clone());
            self.current = Some(shown);
            return Some(action);
        }
        if self.side.as_ref().is_some_and(|s| s.island.id == island.id) {
            let side = Self::side(island.clone(), now, now);
            let action = Action::Side(side.island.clone());
            self.side = Some(side);
            return Some(action);
        }
        None
    }

    fn side(mut island: Island, born: Instant, now: Instant) -> Shown {
        island.dwell_ms = Some(SIDE_DWELL_MS);
        island.tier = Some(IslandTier::Whisper);
        island.shrunk = false;
        Shown {
            island,
            tier: IslandTier::Whisper,
            born,
            since: now,
            dwell_ms: Some(SIDE_DWELL_MS),
        }
    }

    /// The island as it is to be drawn.
    fn prepare(
        mut island: Island,
        env: &Env,
        shrunk: bool,
        resume_ms: Option<u32>,
        born: Instant,
        now: Instant,
    ) -> Shown {
        let tier = gate::tier_of(&island, env.quiet);
        let dwell_ms = match resume_ms {
            Some(ms) if tier != IslandTier::Card => Some(ms),
            _ => gate::dwell_ms(&island, tier),
        };
        island.tier = Some(tier);
        island.dwell_ms = dwell_ms;
        island.shrunk = shrunk;
        Shown {
            island,
            tier: if shrunk { IslandTier::Whisper } else { tier },
            born,
            since: now,
            dwell_ms,
        }
    }

    fn show(&mut self, held: Held, env: &Env, shrunk: bool, now: Instant, out: &mut Vec<Action>) {
        let shown = Self::prepare(held.island, env, shrunk, held.resume_ms, held.born, now);
        out.push(Action::Show(shown.island.clone()));
        self.current = Some(shown);
        self.idle_since = None;
    }

    fn interrupt(
        &mut self,
        held: Held,
        env: &Env,
        shrunk: bool,
        now: Instant,
        out: &mut Vec<Action>,
    ) {
        let old = self.current.take();
        self.show(held, env, shrunk, now, out);
        let Some(old) = old else { return };
        out.push(Action::Clear(old.island.id.clone()));
        // A whisper is stale once interrupted; a peek or card has not had its
        // say and comes back with what was left of its time.
        if old.tier != IslandTier::Whisper {
            let resume_ms = old.remaining_ms(now);
            self.waiting.push(Held {
                island: old.island,
                born: old.born,
                resume_ms,
                resumed: true,
            });
        }
    }

    /// Puts the first waiting event the rules now allow on the stage, urgent
    /// first and then in arrival order. One per call: the next waits a beat.
    fn advance(&mut self, env: &Env, now: Instant, out: &mut Vec<Action>) {
        if self.waiting.is_empty() {
            return;
        }
        if self.current.is_none() && self.idle_since.is_some_and(|t| now < t + GAP) {
            return;
        }
        self.waiting.sort_by(|a, b| {
            b.island
                .priority
                .cmp(&a.island.priority)
                .then(a.born.cmp(&b.born))
        });
        let mut i = 0;
        while i < self.waiting.len() {
            // Judged as if it were not itself in the line.
            match self.decide_waiting(i, env) {
                Decision::Queue(_) => i += 1,
                Decision::Drop(_) => {
                    self.waiting.remove(i);
                }
                decision => {
                    let held = self.waiting.remove(i);
                    match decision {
                        Decision::Show { shrunk } => self.show(held, env, shrunk, now, out),
                        Decision::Interrupt { shrunk } => {
                            self.interrupt(held, env, shrunk, now, out);
                        }
                        _ => {
                            let side = Self::side(held.island, held.born, now);
                            out.push(Action::Side(side.island.clone()));
                            self.side = Some(side);
                        }
                    }
                    return;
                }
            }
        }
    }

    fn announce(&mut self, out: &mut Vec<Action>) {
        let n = self.waiting.len();
        if n != self.told {
            self.told = n;
            out.push(Action::Waiting(n));
        }
    }
}

/// Priority is part of the line's order, so a test can say what came first.
#[cfg(test)]
fn ids(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Show(i) | Action::Side(i) => Some(i.id.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IslandKind, IslandPriority};

    fn alert(id: &str, tier: IslandTier, priority: IslandPriority) -> Island {
        Island {
            id: id.into(),
            kind: IslandKind::Alert,
            priority,
            tier: Some(tier),
            title: id.into(),
            ..Island::default()
        }
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    use IslandPriority::{High, Low, Normal, Urgent};
    use IslandTier::{Card, Peek, Whisper};

    #[test]
    fn a_waiting_event_takes_the_stage_when_the_one_on_it_ends() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("screenshot", Peek, Normal), &env, t);
        let (decision, out) = stage.offer(alert("drive", Peek, Normal), &env, t);
        assert_eq!(decision, Decision::Queue(gate::Hold::Behind));
        assert_eq!(out, [Action::Waiting(1)]);

        stage.end("screenshot", at(t, 4_000));
        assert!(
            stage.tick(&env, at(t, 4_100)).is_empty(),
            "a beat between the two"
        );
        let out = stage.tick(&env, at(t, 4_150));
        assert_eq!(ids(&out), ["drive"]);
        assert!(out.contains(&Action::Waiting(0)));
    }

    #[test]
    fn an_interrupted_peek_comes_back_with_what_was_left_of_its_time() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("download", Peek, Normal), &env, t);
        let (_, out) = stage.offer(alert("call", Card, Urgent), &env, at(t, 1_000));
        assert!(matches!(&out[0], Action::Show(i) if i.id == "call"));
        assert!(out.contains(&Action::Clear("download".into())));

        stage.end("call", at(t, 9_000));
        let out = stage.tick(&env, at(t, 9_200));
        let Some(Action::Show(back)) = out.first() else {
            panic!("the peek resumes: {out:?}");
        };
        assert_eq!(back.id, "download");
        assert_eq!(back.dwell_ms, Some(3_000), "4 s minus the 1 s it had");
    }

    #[test]
    fn an_interrupted_whisper_is_gone_not_resumed() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("caps", Whisper, Normal), &env, t);
        stage.offer(alert("airdrop", Peek, High), &env, at(t, 200));
        assert_eq!(stage.waiting(), 0);
    }

    #[test]
    fn a_whisper_over_a_peek_leaves_the_peek_where_it_is() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("mail", Peek, Normal), &env, t);
        let (decision, out) = stage.offer(alert("wifi", Whisper, Normal), &env, at(t, 500));
        assert_eq!(decision, Decision::Overlay);
        assert!(matches!(&out[0], Action::Side(i) if i.dwell_ms == Some(1_500)));

        // The peek's own end is untouched by the whisper ending.
        stage.end("wifi", at(t, 2_000));
        assert!(stage.is_active());
        stage.end("mail", at(t, 4_000));
        assert!(!stage.is_active());
    }

    /// The failure: a whisper waiting for a card that never ends, and then
    /// popping up a minute later about something long over.
    #[test]
    fn a_waiting_whisper_goes_stale_and_is_never_shown() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("call", Card, Urgent), &env, t);
        stage.offer(alert("charger", Whisper, Normal), &env, t);
        assert_eq!(stage.waiting(), 1);

        let out = stage.tick(&env, at(t, 5_001));
        assert_eq!(out, [Action::Waiting(0)]);
    }

    #[test]
    fn the_line_runs_urgent_first_then_in_arrival_order() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("first", Peek, Normal), &env, t);
        stage.offer(alert("a", Peek, Normal), &env, at(t, 10));
        stage.offer(alert("b", Peek, Normal), &env, at(t, 20));
        stage.offer(alert("battery", Card, Urgent), &env, at(t, 30));
        // `battery` interrupted `first`, which resumes behind the others
        // already waiting only by priority: both normal, `a` arrived earlier.
        stage.end("battery", at(t, 100));
        let order: Vec<String> = (0..3)
            .flat_map(|n| {
                let out = stage.tick(&env, at(t, 300 + n * 1_000));
                let id = ids(&out);
                if let Some(first) = id.first() {
                    stage.end(first, at(t, 300 + n * 1_000));
                }
                id
            })
            .collect();
        assert_eq!(order, ["first", "a", "b"]);
    }

    #[test]
    fn the_same_id_updates_in_place_and_restarts_the_clock() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        let mut volume = alert("hud:volume", Whisper, Normal);
        volume.source = IslandSource::Hud;
        volume.level = Some(30);
        stage.offer(volume.clone(), &env, t);
        volume.level = Some(40);
        let (_, out) = stage.offer(volume, &env, at(t, 1_000));
        assert!(matches!(&out[0], Action::Show(i) if i.level == Some(40)));
        assert_eq!(stage.waiting(), 0, "twenty presses are one whisper");
        // 1.8 s dwell restarted at 1.0 s, plus the grace: still up well after
        // the first would have gone.
        assert!(stage.tick(&env, at(t, 10_000)).is_empty());
        assert!(!stage.tick(&env, at(t, 1_000 + 1_800 + 8_001)).is_empty());
    }

    #[test]
    fn a_duplicate_waiting_in_the_line_is_replaced_by_the_newer() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("call", Card, Urgent), &env, t);
        let mut level = alert("hud:volume", Whisper, Normal);
        level.level = Some(10);
        stage.offer(level.clone(), &env, t);
        level.level = Some(60);
        stage.offer(level, &env, at(t, 100));
        assert_eq!(
            stage.waiting(),
            1,
            "+N counts distinct items, never repeats"
        );
    }

    #[test]
    fn opening_a_panel_lets_go_of_news_but_holds_a_card_for_after() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("battery", Card, Urgent), &env, t);
        let out = stage.panel_opened(at(t, 500));
        assert!(out.contains(&Action::Clear("battery".into())));
        assert_eq!(stage.waiting(), 1, "an unanswered card is not lost");

        let open = Env {
            panel_open: true,
            ..Env::default()
        };
        assert!(
            stage.tick(&open, at(t, 1_000)).is_empty(),
            "held while open"
        );
        let out = stage.tick(&env, at(t, 1_200));
        assert_eq!(ids(&out), ["battery"]);

        let mut stage = Stage::new();
        stage.offer(alert("shot", Peek, Normal), &env, t);
        stage.panel_opened(at(t, 500));
        assert_eq!(stage.waiting(), 0, "a peek was only news");
    }

    /// The failure: a takeover whose webview never reported its end — the
    /// panel opened over it, the window closed — blocking every event after.
    #[test]
    fn a_takeover_nobody_ended_is_let_go_past_its_dwell_and_grace() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("shot", Peek, Normal), &env, t);
        assert!(stage.tick(&env, at(t, 4_000 + 8_000)).is_empty());
        let out = stage.tick(&env, at(t, 4_000 + 8_001));
        assert_eq!(out, [Action::Clear("shot".into())]);
        assert!(!stage.is_active());
    }

    #[test]
    fn a_card_is_never_let_go_by_the_clock() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        stage.offer(alert("battery", Card, Urgent), &env, t);
        assert!(stage.tick(&env, at(t, 3_600_000)).is_empty());
    }

    #[test]
    fn a_realistic_morning_ends_with_the_stage_empty() {
        let t = Instant::now();
        let env = Env::default();
        let mut stage = Stage::new();
        let mut shown = Vec::new();
        let mut offer = |stage: &mut Stage, island: Island, ms: u64| {
            shown.extend(ids(&stage.offer(island, &env, at(t, ms)).1));
        };
        // A screenshot, a drive and the 20% chip within a second; a Wi-Fi
        // whisper (low) turns up while all of them are on screen.
        offer(&mut stage, alert("shot", Peek, Normal), 0);
        offer(&mut stage, alert("drive", Peek, Normal), 400);
        offer(&mut stage, alert("battery-low", Whisper, Normal), 800);
        offer(&mut stage, alert("wifi", Whisper, Low), 900);
        assert_eq!(
            stage.waiting(),
            1,
            "the drive waits; the low one is dropped"
        );
        assert_eq!(shown, ["shot", "battery-low"], "the whisper rides the peek");

        stage.end("battery-low", at(t, 2_300));
        stage.end("shot", at(t, 4_000));
        let out = stage.tick(&env, at(t, 4_200));
        assert_eq!(ids(&out), ["drive"]);
        stage.end("drive", at(t, 8_000));
        assert!(!stage.is_active());
    }

    #[test]
    fn an_urgent_card_waits_for_a_panel_it_cannot_draw_over() {
        let t = Instant::now();
        let open = Env {
            panel_open: true,
            ..Env::default()
        };
        let mut stage = Stage::new();
        let (decision, out) = stage.offer(alert("battery", Card, Urgent), &open, t);
        assert_eq!(decision, Decision::Queue(gate::Hold::PanelOpen));
        assert_eq!(out, [Action::Waiting(1)]);
        let out = stage.tick(&Env::default(), at(t, 100));
        assert_eq!(ids(&out), ["battery"], "and shows the moment it closes");
    }
}
