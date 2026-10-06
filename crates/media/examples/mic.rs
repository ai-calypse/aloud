//! Watches the default microphone's mute, and can toggle it to prove the watch.
//!
//!   cargo run -p aloud-media --example mic -- [seconds]          # watch while you mute in System Settings or the app
//!   cargo run -p aloud-media --example mic -- toggle             # mute then unmute through `set_mic_muted`, and restore
//!
//! Prints `mic muted` / `mic on` as the listener fires. CoreAudio calls the
//! listener on its own thread, so this needs no run loop.

use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let arg = std::env::args().nth(1);
    let original = aloud_media::sound::mic_muted();
    println!("default input muted: {original:?}");
    let Some(original) = original else {
        println!("this microphone has neither a mute switch nor a level: nothing to watch");
        return Ok(());
    };

    aloud_media::sound::watch_mic(|muted| {
        println!("listener: {}", if muted { "mic muted" } else { "mic on" })
    })?;

    if arg.as_deref() == Some("toggle") {
        for muted in [!original, original] {
            println!("set_mic_muted({muted})");
            aloud_media::sound::set_mic_muted(muted)?;
            std::thread::sleep(Duration::from_millis(600));
        }
        println!(
            "restored: {:?}",
            aloud_media::sound::mic_muted().expect("had a control a moment ago")
        );
    } else {
        let seconds = arg.and_then(|a| a.parse().ok()).unwrap_or(15);
        println!("watching for {seconds}s");
        std::thread::sleep(Duration::from_secs(seconds));
    }
    Ok(())
}
