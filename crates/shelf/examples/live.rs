//! Real-hardware check for the Shelf watchers, which unit tests cannot prove:
//! FSEvents behaves differently on `/Volumes` and on the Desktop than in a
//! temp dir.
//!
//!   cargo run -p aloud-shelf --example live -- drives     # then mount/eject a disk
//!   cargo run -p aloud-shelf --example live -- shots [dir] # then press ⇧⌘3
//!   cargo run -p aloud-shelf --example live -- downloads  # then download something
//!   cargo run -p aloud-shelf --example live -- eject /Volumes/<name>

use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("drives") => {
            println!("now: {:#?}", aloud_shelf::drives::list());
            let _w = aloud_shelf::drives::watch(|c| println!("change: {c:?}"))?;
            std::thread::sleep(Duration::from_secs(120));
        }
        Some("shots") => {
            let mut loc = aloud_shelf::shots::Location::read();
            if let Some(dir) = args.get(1) {
                loc.dir = dir.into();
            }
            println!(
                "watching {loc:?}\nrecent: {:#?}",
                aloud_shelf::shots::recent(&loc, 3)
            );
            let _w = aloud_shelf::shots::watch(loc, |s| println!("new shot: {s:?}"))?;
            std::thread::sleep(Duration::from_secs(120));
        }
        Some("downloads") => {
            let dir = aloud_shelf::downloads::dir().expect("no home");
            println!("recent: {:#?}", aloud_shelf::downloads::recent(&dir, 3));
            let _w = aloud_shelf::downloads::watch(&dir, |d| println!("finished: {d:?}"))?;
            std::thread::sleep(Duration::from_secs(120));
        }
        Some("eject") => {
            let mount = args.get(1).expect("usage: eject /Volumes/<name>");
            aloud_shelf::drives::eject(std::path::Path::new(mount))?;
            println!("ejected");
        }
        _ => eprintln!("usage: live drives|shots|downloads|eject <mount>"),
    }
    Ok(())
}
