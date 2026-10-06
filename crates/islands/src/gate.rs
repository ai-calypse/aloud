//! The takeover gate: whether an event may borrow the notch now, wait for its
//! turn, or is not worth showing at all.
//!
//! Pure, so every rule is a test: the rules are the design in
//! `docs/design/notch-events.html` ("Priority and queue model"), and each is
//! easy to get wrong without anyone noticing — a card dropped under Quiet, a
//! volume bar queued behind a call. Nothing here knows about windows or
//! clocks; [`crate::stage::Stage`] holds the state and asks.

use std::time::Duration;

use crate::{Island, IslandPriority, IslandSource, IslandTier};

/// A keystroke this recently means the user is typing, which shrinks what would
/// land on top of their text.
pub const TYPING_MS: u64 = 1_800;

/// What is on screen, as the rules see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Current {
    /// As drawn: a peek shrunk to a side capsule counts as a whisper.
    pub tier: IslandTier,
    pub priority: IslandPriority,
    /// A peek the user opened by pointing at it: its top row is covered.
    pub expanded: bool,
}

/// Everything the decision depends on besides the event itself.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub current: Option<Current>,
    /// Something else is already waiting for its turn.
    pub waiting: bool,
    /// The user is in a panel (not a takeover of ours).
    pub panel_open: bool,
    pub typing: bool,
    /// A macOS Focus is on and the user asked Aloud to respect it.
    pub focus_on: bool,
    /// Quiet mode: everything one tier smaller.
    pub quiet: bool,
    /// Sources the user switched off in Settings.
    pub muted: Vec<IslandSource>,
}

/// What to do with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing is showing: take the notch. `shrunk`: as a side capsule,
    /// because the user is typing.
    Show {
        shrunk: bool,
    },
    /// Take the notch from what is showing.
    Interrupt {
        shrunk: bool,
    },
    /// A whisper rides the top row of the peek on screen; the peek stays.
    Overlay,
    /// A key press shows as a small capsule under a card, which is never
    /// replaced.
    Echo,
    Queue(Hold),
    Drop(Skip),
}

/// Why an event waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    PanelOpen,
    Typing,
    /// Something showing outranks it, or is not allowed to be interrupted.
    Behind,
}

/// Why an event is not shown at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Its Settings switch is off.
    Muted,
    Focus,
    /// Low priority, and the user is busy or something is showing.
    Busy,
}

/// The tier it is shown at: its own, one smaller under Quiet — except an urgent
/// card (a call, an approval, battery under 10%), which stays a card.
pub fn tier_of(island: &Island, quiet: bool) -> IslandTier {
    let tier = island.takeover_tier().unwrap_or(IslandTier::Peek);
    if quiet && !(tier == IslandTier::Card && island.priority == IslandPriority::Urgent) {
        tier.quieter()
    } else {
        tier
    }
}

/// How long a takeover stays before leaving on its own; None for a card, which
/// stays until answered. Whispers are 1.2–2 s whatever a producer asks for.
pub fn dwell_ms(island: &Island, tier: IslandTier) -> Option<u32> {
    match tier {
        IslandTier::Card => None,
        IslandTier::Whisper => Some(island.dwell_ms.unwrap_or(1_800).clamp(1_200, 2_000)),
        IslandTier::Peek => Some(island.dwell_ms.unwrap_or(4_000).max(2_000)),
    }
}

/// How long a waiting event stays worth showing: the moment passes, and
/// Notification Center still has it. Urgent never goes stale.
pub fn stale_after(tier: IslandTier, priority: IslandPriority) -> Option<Duration> {
    match (tier, priority) {
        (_, IslandPriority::Urgent) => None,
        (IslandTier::Whisper, _) => Some(Duration::from_secs(5)),
        _ => Some(Duration::from_secs(30)),
    }
}

/// The decision, in the design's order: switches and Focus, then the busy
/// rules, then who outranks whom.
pub fn decide(island: &Island, ctx: &Ctx) -> Decision {
    let tier = tier_of(island, ctx.quiet);
    let priority = island.priority;
    let urgent = priority == IslandPriority::Urgent;

    if ctx.muted.contains(&island.source) {
        return Decision::Drop(Skip::Muted);
    }
    // Aloud's own and key presses are what the user asked for; macOS's
    // ordinary news can wait out a Focus.
    if ctx.focus_on
        && island.source.is_system()
        && !island.source.is_direct()
        && priority <= IslandPriority::Normal
    {
        return Decision::Drop(Skip::Focus);
    }
    // Low is only ever for an empty, idle notch, and never queued.
    if priority == IslandPriority::Low
        && (ctx.current.is_some() || ctx.waiting || ctx.typing || ctx.panel_open)
    {
        return Decision::Drop(Skip::Busy);
    }
    if ctx.panel_open && !urgent {
        return Decision::Queue(Hold::PanelOpen);
    }
    // Whispers sit at the notch's height, not over the user's text.
    let mut shrunk = false;
    if ctx.typing && tier != IslandTier::Whisper && !urgent {
        match priority {
            IslandPriority::High => shrunk = true,
            _ => return Decision::Queue(Hold::Typing),
        }
    }

    let Some(current) = ctx.current else {
        return Decision::Show { shrunk };
    };
    if urgent {
        return if current.priority == IslandPriority::Urgent {
            Decision::Queue(Hold::Behind)
        } else {
            Decision::Interrupt { shrunk }
        };
    }
    if tier == IslandTier::Whisper {
        let direct = island.source.is_direct();
        return match current.tier {
            IslandTier::Peek if !current.expanded => Decision::Overlay,
            IslandTier::Whisper if direct || priority == IslandPriority::High => {
                Decision::Interrupt { shrunk }
            }
            IslandTier::Whisper => Decision::Queue(Hold::Behind),
            // A card, or a peek that is open: only a key press gets through,
            // and only as an echo.
            _ if direct => Decision::Echo,
            _ => Decision::Queue(Hold::Behind),
        };
    }
    if priority == IslandPriority::High && current.tier == IslandTier::Whisper {
        return Decision::Interrupt { shrunk };
    }
    Decision::Queue(Hold::Behind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IslandKind;

    fn event(tier: IslandTier, priority: IslandPriority) -> Island {
        Island {
            id: "e".into(),
            kind: IslandKind::Alert,
            priority,
            tier: Some(tier),
            ..Island::default()
        }
    }

    fn key(mut island: Island) -> Island {
        island.source = IslandSource::Hud;
        island
    }

    fn system(mut island: Island) -> Island {
        island.source = IslandSource::Power;
        island
    }

    fn showing(tier: IslandTier, priority: IslandPriority) -> Ctx {
        Ctx {
            current: Some(Current {
                tier,
                priority,
                expanded: false,
            }),
            ..Ctx::default()
        }
    }

    use IslandPriority::{High, Low, Normal, Urgent};
    use IslandTier::{Card, Peek, Whisper};

    #[test]
    fn an_alert_pushed_before_tiers_is_a_peek() {
        let old = Island {
            kind: IslandKind::Alert,
            ..Island::default()
        };
        assert_eq!(tier_of(&old, false), Peek);
        let json = r#"{"id":"x","kind":"alert","priority":"normal","icon":null,"title":"t",
            "detail":null,"actions":[],"expires":null}"#;
        let parsed: Island = serde_json::from_str(json).expect("an old push still parses");
        assert_eq!(parsed.takeover_tier(), Some(Peek));
        assert_eq!(parsed.source, IslandSource::Own);
    }

    #[test]
    fn an_empty_notch_shows_whatever_arrives() {
        for tier in [Whisper, Peek, Card] {
            assert_eq!(
                decide(&event(tier, Normal), &Ctx::default()),
                Decision::Show { shrunk: false }
            );
        }
    }

    #[test]
    fn urgent_interrupts_anything_but_another_urgent() {
        let call = event(Card, Urgent);
        for tier in [Whisper, Peek, Card] {
            assert_eq!(
                decide(&call, &showing(tier, High)),
                Decision::Interrupt { shrunk: false }
            );
        }
        assert_eq!(
            decide(&call, &showing(Card, Urgent)),
            Decision::Queue(Hold::Behind),
            "a second urgent waits behind the first"
        );
    }

    #[test]
    fn high_interrupts_a_whisper_but_not_a_peek_or_card() {
        let airdrop = event(Peek, High);
        assert_eq!(
            decide(&airdrop, &showing(Whisper, Normal)),
            Decision::Interrupt { shrunk: false }
        );
        assert_eq!(
            decide(&airdrop, &showing(Peek, Normal)),
            Decision::Queue(Hold::Behind)
        );
        assert_eq!(
            decide(&airdrop, &showing(Card, Urgent)),
            Decision::Queue(Hold::Behind)
        );
    }

    #[test]
    fn normal_waits_its_turn_behind_anything() {
        for current in [Whisper, Peek, Card] {
            assert_eq!(
                decide(&event(Peek, Normal), &showing(current, Normal)),
                Decision::Queue(Hold::Behind)
            );
        }
        assert_eq!(
            decide(&event(Whisper, Normal), &showing(Whisper, Normal)),
            Decision::Queue(Hold::Behind),
            "a plain whisper does not cut in on another"
        );
    }

    #[test]
    fn low_shows_only_when_the_notch_is_idle_and_is_never_queued() {
        let wifi = event(Whisper, Low);
        assert_eq!(
            decide(&wifi, &Ctx::default()),
            Decision::Show { shrunk: false }
        );
        let busy = [
            showing(Whisper, Normal),
            Ctx {
                waiting: true,
                ..Ctx::default()
            },
            Ctx {
                typing: true,
                ..Ctx::default()
            },
            Ctx {
                panel_open: true,
                ..Ctx::default()
            },
        ];
        for ctx in busy {
            assert_eq!(decide(&wifi, &ctx), Decision::Drop(Skip::Busy));
        }
    }

    #[test]
    fn a_whisper_rides_the_top_row_of_a_peek_instead_of_replacing_it() {
        assert_eq!(
            decide(&event(Whisper, Normal), &showing(Peek, Normal)),
            Decision::Overlay
        );
    }

    /// The failure: an open peek's top row is the app and the ×; a whisper
    /// painted there would sit under the pointer's own hover.
    #[test]
    fn a_whisper_does_not_overlay_a_peek_the_user_has_opened() {
        let mut ctx = showing(Peek, Normal);
        ctx.current.as_mut().unwrap().expanded = true;
        assert_eq!(
            decide(&event(Whisper, Normal), &ctx),
            Decision::Queue(Hold::Behind)
        );
    }

    #[test]
    fn over_a_card_only_a_key_press_shows_and_as_an_echo() {
        let card = showing(Card, Urgent);
        assert_eq!(decide(&key(event(Whisper, Normal)), &card), Decision::Echo);
        assert_eq!(
            decide(&event(Whisper, Normal), &card),
            Decision::Queue(Hold::Behind),
            "other whispers wait, and go stale"
        );
    }

    #[test]
    fn a_key_press_replaces_a_whisper_at_once() {
        assert_eq!(
            decide(&key(event(Whisper, Normal)), &showing(Whisper, Normal)),
            Decision::Interrupt { shrunk: false }
        );
    }

    #[test]
    fn typing_shrinks_a_high_peek_and_holds_a_normal_one() {
        let typing = |current: Option<Current>| Ctx {
            current,
            typing: true,
            ..Ctx::default()
        };
        assert_eq!(
            decide(&event(Peek, High), &typing(None)),
            Decision::Show { shrunk: true }
        );
        assert_eq!(
            decide(&event(Card, High), &typing(None)),
            Decision::Show { shrunk: true }
        );
        assert_eq!(
            decide(&event(Peek, Normal), &typing(None)),
            Decision::Queue(Hold::Typing)
        );
    }

    #[test]
    fn typing_does_not_stop_a_whisper_or_an_urgent_card() {
        let ctx = Ctx {
            typing: true,
            ..Ctx::default()
        };
        assert_eq!(
            decide(&event(Whisper, Normal), &ctx),
            Decision::Show { shrunk: false }
        );
        assert_eq!(
            decide(&event(Card, Urgent), &ctx),
            Decision::Show { shrunk: false },
            "a call is shown in full however fast you type"
        );
    }

    #[test]
    fn with_the_panel_open_only_urgent_gets_through() {
        let ctx = Ctx {
            panel_open: true,
            ..Ctx::default()
        };
        for (tier, priority) in [(Whisper, Normal), (Peek, High), (Card, High)] {
            assert_eq!(
                decide(&event(tier, priority), &ctx),
                Decision::Queue(Hold::PanelOpen)
            );
        }
        assert_eq!(
            decide(&event(Card, Urgent), &ctx),
            Decision::Show { shrunk: false }
        );
    }

    #[test]
    fn quiet_mode_drops_one_tier_and_only_an_urgent_card_stays_a_card() {
        assert_eq!(tier_of(&event(Card, High), true), Peek);
        assert_eq!(tier_of(&event(Peek, Normal), true), Whisper);
        assert_eq!(tier_of(&event(Whisper, Normal), true), Whisper);
        assert_eq!(tier_of(&event(Card, Urgent), true), Card);
        assert_eq!(tier_of(&event(Card, High), false), Card);
    }

    /// The failure: Quiet dropping the tier but not the rules — a quieted card
    /// still going through the card path (sticky, no dwell).
    #[test]
    fn a_quieted_card_leaves_on_its_own_like_a_peek() {
        let failed = event(Card, High);
        assert_eq!(dwell_ms(&failed, tier_of(&failed, false)), None);
        assert_eq!(dwell_ms(&failed, tier_of(&failed, true)), Some(4_000));
    }

    #[test]
    fn focus_holds_normal_and_low_system_events_only() {
        let focus = Ctx {
            focus_on: true,
            ..Ctx::default()
        };
        assert_eq!(
            decide(&system(event(Peek, Normal)), &focus),
            Decision::Drop(Skip::Focus)
        );
        assert_eq!(
            decide(&system(event(Whisper, Low)), &focus),
            Decision::Drop(Skip::Focus)
        );
        assert_eq!(
            decide(&system(event(Peek, High)), &focus),
            Decision::Show { shrunk: false }
        );
        assert_eq!(
            decide(&event(Peek, Normal), &focus),
            Decision::Show { shrunk: false },
            "Aloud's own are never held"
        );
        assert_eq!(
            decide(&key(event(Whisper, Normal)), &focus),
            Decision::Show { shrunk: false },
            "a key press was asked for"
        );
    }

    #[test]
    fn a_source_the_user_switched_off_is_dropped() {
        let ctx = Ctx {
            muted: vec![IslandSource::Power],
            ..Ctx::default()
        };
        assert_eq!(
            decide(&system(event(Card, Urgent)), &ctx),
            Decision::Drop(Skip::Muted)
        );
        assert_eq!(
            decide(&event(Card, Urgent), &ctx),
            Decision::Show { shrunk: false }
        );
    }

    #[test]
    fn dwell_is_whispers_1_2_to_2s_peeks_4s_cards_never() {
        let mut whisper = event(Whisper, Normal);
        assert_eq!(dwell_ms(&whisper, Whisper), Some(1_800));
        whisper.dwell_ms = Some(300);
        assert_eq!(dwell_ms(&whisper, Whisper), Some(1_200));
        whisper.dwell_ms = Some(9_000);
        assert_eq!(dwell_ms(&whisper, Whisper), Some(2_000));
        assert_eq!(dwell_ms(&event(Peek, Normal), Peek), Some(4_000));
        assert_eq!(dwell_ms(&event(Card, High), Card), None);
    }

    #[test]
    fn a_waiting_whisper_goes_stale_in_5s_a_peek_in_30s_urgent_never() {
        assert_eq!(stale_after(Whisper, Normal), Some(Duration::from_secs(5)));
        assert_eq!(stale_after(Peek, Normal), Some(Duration::from_secs(30)));
        assert_eq!(stale_after(Card, High), Some(Duration::from_secs(30)));
        assert_eq!(stale_after(Card, Urgent), None);
    }
}
