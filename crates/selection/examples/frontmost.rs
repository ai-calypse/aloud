//! Prints the currently focused app's name.
//!
//! Unit tests can only assert "does not panic with no focus"; the real check is
//! whether the AX read returns the right app on real hardware. Run it, then
//! click into Slack / your editor / a browser and run it again.
//!
//!   cargo run -p aloud-selection --example frontmost
//!
//! Needs Accessibility permission (the same one hotkeys use).

fn main() {
    match aloud_selection::frontmost_app() {
        Some(app) => println!("frontmost app: {app}"),
        None => println!("could not determine the frontmost app (no focus or permission denied)"),
    }
}
