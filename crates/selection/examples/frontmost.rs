//! Prints the currently focused app's name, and says which step failed if it
//! cannot.
//!
//! Unit tests can only assert "does not panic with no focus"; the real check is
//! whether the read returns the right app on real hardware. Run it, then click
//! into Slack / your editor / a browser and run it again.
//!
//!   cargo run -p aloud-selection --example frontmost
//!
//! Needs Accessibility permission (the same one hotkeys use). The staged
//! output exists because this returned None for *every* app for a long time —
//! it read `AXTitle`, which is empty on an application element — and a bare
//! "could not determine" gave no way to tell that from a permission denial.

fn main() {
    println!("AX trusted: {}", aloud_selection::debug::is_trusted());

    match aloud_selection::debug::frontmost_pid() {
        Some(pid) => {
            println!("focused pid: {pid}");
            match aloud_selection::debug::executable_path(pid) {
                Some(path) => println!("executable:  {path}"),
                None => println!("executable:  <proc_pidpath failed>"),
            }
        }
        None => {
            let err = aloud_selection::debug::frontmost_error();
            let hint = match err {
                -25204 => " (kAXErrorCannotComplete — is the screen locked?)",
                -25211 => " (kAXErrorAPIDisabled — grant Accessibility)",
                _ => "",
            };
            println!("focused pid: <no focused application, AX error {err}{hint}>");
        }
    }

    match aloud_selection::frontmost_app() {
        Some(app) => println!(
            "frontmost app: {}  bundle id: {}",
            app.name,
            app.bundle_id.as_deref().unwrap_or("<none>")
        ),
        None => println!("frontmost app: <none>"),
    }
}
