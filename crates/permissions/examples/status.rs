//! Prints the real TCC answers for this process. Unit tests cannot: the
//! result depends on what the user granted, so the only proof is running it.
//!
//! `cargo run -p aloud-permissions --example status`
//!
//! Note that the answers belong to *this binary*, not to Aloud.app — TCC keys
//! on the code signature, so a bare cargo binary and the bundled app have
//! separate grants.

fn main() {
    for status in aloud_permissions::all() {
        println!("{:?}: {}", status.permission, status.granted);
    }
}
