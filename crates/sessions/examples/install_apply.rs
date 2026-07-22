//! Installs or removes our hooks in a real settings file.
//!
//! ```sh
//! cargo run -p aloud-sessions --example install_apply -- install
//! cargo run -p aloud-sessions --example install_apply -- uninstall
//! ```
//!
//! The same pure functions the app's commands call, reachable without launching
//! the GUI. Writes through a temp file and a rename, so a crash mid-write
//! cannot leave another tool's config truncated.
use aloud_sessions::install;

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let home = std::env::var("HOME").expect("HOME");
    let path = std::path::PathBuf::from(&home).join(".claude/settings.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let before: serde_json::Value = serde_json::from_str(&text).expect("parse settings");

    let command =
        format!("'{home}/Library/Application Support/Aloud/bin/aloud-hook' --source claude");
    let after = match mode.as_str() {
        // Approvals off: another tool owns PermissionRequest here, and step 4
        // is what would answer one anyway.
        "install" => install::install(&before, &command, false),
        "uninstall" => install::uninstall(&before),
        _ => {
            eprintln!("usage: install_apply <install|uninstall>");
            return;
        }
    };

    let temp = path.with_extension("aloud-tmp");
    std::fs::write(
        &temp,
        format!("{}\n", serde_json::to_string_pretty(&after).unwrap()),
    )
    .expect("write");
    std::fs::rename(&temp, &path).expect("rename");
    println!(
        "{mode}: our events are now {:?}",
        install::installed_events(&after)
    );
}
