//! What installing would do to a real settings file, without writing it.
//!
//! ```sh
//! cargo run -p aloud-sessions --example install_dryrun -- ~/.claude/settings.json
//! ```
//!
//! Reads only. The file named is never modified — the point is to see the
//! conflict and the diff before anything touches the user's config.
use aloud_sessions::install::{self, Conflict};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: install_dryrun <settings.json>");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read settings");
    let before: serde_json::Value = serde_json::from_str(&text).expect("parse settings");

    match install::permission_conflict(&before) {
        Conflict::Foreign(command) => {
            println!("PermissionRequest is owned by another tool:\n  {command}\n");
            println!("-> approvals stay off unless the user takes it over.\n");
        }
        Conflict::None => println!("PermissionRequest is free.\n"),
    }

    let command = "'~/Library/Application Support/Aloud/bin/aloud-hook' --source claude";
    let after = install::install(&before, command, false);
    println!(
        "our events after install: {:?}",
        install::installed_events(&after)
    );

    // The claim that matters: everyone else's entries survive, and uninstalling
    // puts the file back exactly as it was found.
    let others = |v: &serde_json::Value| -> Vec<String> {
        v["hooks"]
            .as_object()
            .map(|events| {
                events
                    .iter()
                    .flat_map(|(event, list)| {
                        list.as_array().into_iter().flatten().flat_map(move |m| {
                            m["hooks"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(move |h| {
                                    let c = h["command"].as_str()?;
                                    (!c.contains("aloud-hook"))
                                        .then(|| format!("{event}: {}", &c[..c.len().min(60)]))
                                })
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut before_others = others(&before);
    let mut after_others = others(&after);
    before_others.sort();
    after_others.sort();
    println!("\nother tools' hooks before: {}", before_others.len());
    println!("other tools' hooks after:  {}", after_others.len());
    println!("preserved exactly: {}", before_others == after_others);
    println!(
        "uninstall restores the file: {}",
        install::uninstall(&after) == before
    );
}
