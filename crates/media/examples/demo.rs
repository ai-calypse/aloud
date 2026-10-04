//! Drives the real players, devices and network — what the unit tests cannot.
//!
//!   cargo run -p aloud-media --example demo -- now        # Spotify/Music, read once
//!   cargo run -p aloud-media --example demo -- sound      # output and input devices
//!   cargo run -p aloud-media --example demo -- switch output <id>   # make a device the default
//!   cargo run -p aloud-media --example demo -- mic        # mutes the mic 2s, then restores it
//!   cargo run -p aloud-media --example demo -- devices    # Bluetooth batteries
//!   cargo run -p aloud-media --example demo -- connect aa:bb:cc:dd:ee:ff   # a paired device
//!   cargo run -p aloud-media --example demo -- lyrics "Blinding Lights" "The Weeknd" 200

use aloud_media::{devices, lyrics, now_playing, sound, NowPlaying, PlayState, Player};
use std::{thread::sleep, time::Duration};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("now") => {
            let reading = now_playing::read();
            println!("{reading:#?}");
            if let Some(now) = reading.now {
                println!(
                    "artwork: {}",
                    aloud_media::artwork(&now)
                        .await
                        .map_or("none".into(), |u| format!("{} bytes", u.len()))
                );
            }
        }
        Some("switch") => {
            let dir = if args[1] == "input" {
                sound::Direction::Input
            } else {
                sound::Direction::Output
            };
            sound::set_default(dir, args[2].parse()?)?;
            println!("{:#?}", sound::state());
        }
        Some("sound") => println!("{:#?}", sound::state()),
        Some("mic") => {
            println!("muted now: {:?}", sound::mic_muted());
            sound::set_mic_muted(true)?;
            println!("after mute: {:?}", sound::mic_muted());
            sleep(Duration::from_secs(2));
            sound::set_mic_muted(false)?;
            println!("after unmute: {:?}", sound::mic_muted());
        }
        Some("connect") => println!("{:?}", devices::connect(&args[1])),
        Some("devices") => println!("{:#?}", devices::list()?),
        Some("lyrics") => {
            let now = NowPlaying {
                player: Player::Spotify,
                state: PlayState::Playing,
                title: args[1].clone(),
                artist: args[2].clone(),
                album: args.get(4).cloned().unwrap_or_default(),
                duration_ms: args[3].parse::<u32>()? * 1000,
                position_ms: 0,
                artwork_url: None,
            };
            match lyrics::lyrics(&now).await? {
                Some(l) => {
                    println!(
                        "{} timed lines, instrumental={}",
                        l.lines.len(),
                        l.instrumental
                    );
                    for line in l.lines.iter().take(6) {
                        println!("  {:>6} ms  {}", line.ms, line.text);
                    }
                }
                None => println!("no lyrics"),
            }
        }
        _ => println!("see the doc comment for subcommands"),
    }
    Ok(())
}
