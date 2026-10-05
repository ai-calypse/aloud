//! Drives the takeover stage with a realistic stretch of a working morning and
//! prints every decision, the way the app's log would. No window, no clock:
//! the example passes its own `now`, so the run is instant and repeatable.
//!
//! ```sh
//! cargo run -p aloud-islands --example takeovers
//! ```

use std::time::{Duration, Instant};

use aloud_islands::stage::{Action, Env, Stage};
use aloud_islands::{Island, IslandKind, IslandPriority, IslandSource, IslandTier};

fn event(id: &str, tier: IslandTier, priority: IslandPriority, source: IslandSource) -> Island {
    Island {
        id: id.into(),
        kind: IslandKind::Alert,
        priority,
        tier: Some(tier),
        title: id.into(),
        source,
        ..Island::default()
    }
}

fn main() {
    let t0 = Instant::now();
    let mut stage = Stage::new();
    let mut env = Env::default();
    let at = |ms: u64| t0 + Duration::from_millis(ms);

    let say = |ms: u64, what: &str, actions: &[Action], waiting: usize| {
        let shown: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Show(i) => Some(format!("show {}", i.id)),
                Action::Side(i) => Some(format!("side {}", i.id)),
                Action::Clear(id) => Some(format!("clear {id}")),
                Action::Waiting(_) => None,
            })
            .collect();
        println!("{ms:>6} ms  {what:<34} {:<34} +{waiting}", shown.join(", "));
    };

    use IslandPriority::{High, Low, Normal, Urgent};
    use IslandTier::{Card, Peek, Whisper};

    // A screenshot lands; a drive, the 20% whisper and a finished download follow.
    let steps: [(u64, &str, Island); 9] = [
        (
            0,
            "screenshot saved",
            event("shot", Peek, Normal, IslandSource::Banners),
        ),
        (
            400,
            "drive connected",
            event("drive", Peek, Normal, IslandSource::Connectivity),
        ),
        (
            800,
            "battery 20%",
            event("battery-low", Whisper, Normal, IslandSource::Power),
        ),
        (
            900,
            "download finished (low)",
            event("download", Whisper, Low, IslandSource::Banners),
        ),
        (
            1_000,
            "volume key",
            event("hud:volume", Whisper, Normal, IslandSource::Hud),
        ),
        (
            1_030,
            "volume key again",
            event("hud:volume", Whisper, Normal, IslandSource::Hud),
        ),
        (
            2_000,
            "timer done (high)",
            event("timer", Peek, High, IslandSource::Own),
        ),
        (
            2_500,
            "battery 9% (urgent card)",
            event("battery", Card, Urgent, IslandSource::Power),
        ),
        (
            2_600,
            "caps lock (plain whisper)",
            event("caps", Whisper, Normal, IslandSource::Hud),
        ),
    ];
    for (ms, what, island) in steps {
        let (decision, actions) = stage.offer(island, &env, at(ms));
        println!("        {decision:?}");
        say(ms, what, &actions, stage.waiting());
    }

    // The user types; a high card shrinks, and a Focus holds ordinary news.
    env.typing = true;
    let (decision, actions) = stage.offer(
        event("airdrop", Peek, High, IslandSource::Connectivity),
        &env,
        at(3_000),
    );
    println!("        {decision:?}");
    say(3_000, "AirDrop while typing", &actions, stage.waiting());
    env.typing = false;
    env.focus_on = true;
    let (decision, actions) = stage.offer(
        event("wifi", Whisper, Normal, IslandSource::Connectivity),
        &env,
        at(3_100),
    );
    println!("        {decision:?}");
    say(
        3_100,
        "Wi-Fi joined during Focus",
        &actions,
        stage.waiting(),
    );
    env.focus_on = false;

    // Each takeover is answered or times out in turn, and the line drains in
    // order: the urgent card, then high, then arrival order. What was
    // interrupted comes back with the time it had left.
    let mut ms = 9_000;
    while let Some(id) = stage.on_stage().map(str::to_owned) {
        stage.end(&id, at(ms));
        let actions = stage.tick(&env, at(ms + 700));
        say(ms, &format!("{id} ended"), &actions, stage.waiting());
        ms += 4_200;
    }
    println!("active at the end: {}", stage.is_active());
}
