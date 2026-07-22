//! Prints the real TCC answers for this process.
//!
//! `cargo run -p aloud-permissions --example status`
//!
//! The answers belong to *this binary*, not Aloud.app: TCC keys on the code
//! signature, so a cargo binary and the bundled app have separate grants.

fn main() {
    for status in aloud_permissions::all() {
        println!("{:?}: {}", status.permission, status.granted);
    }
}
