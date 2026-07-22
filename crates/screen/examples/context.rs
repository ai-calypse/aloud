//! Prints the screen context the cleanup prompt would see, and captures a
//! screenshot to a file so it can actually be looked at.
//!
//! The questions that matter: does the summary name the app the user is really
//! in, and does the image contain their windows or just the wallpaper — the
//! latter being what a denied grant produces, since `screencapture` still
//! exits 0.
//!
//!   cargo run -p aloud-screen --example context
//!
//! Needs Accessibility for the summary and Screen Recording for the image.
//! Click into Mail or your editor, then run it again.

fn main() {
    let granted = aloud_permissions::is_granted(aloud_permissions::Permission::ScreenRecording);
    println!("screen recording granted: {granted}");

    let summary = aloud_screen::summary();
    if summary.is_empty() {
        println!("summary: <nothing known — is Accessibility granted?>");
    } else {
        println!("summary: {summary}");
    }

    let started = std::time::Instant::now();
    match aloud_screen::screenshot_data_uri(aloud_screen::DEFAULT_MAX_DIMENSION) {
        Some(uri) => {
            let b64 = uri.split_once(",").map(|(_, b)| b).unwrap_or_default();
            println!(
                "screenshot: {} chars of data URI in {} ms",
                uri.len(),
                started.elapsed().as_millis()
            );
            // Written out because the only real check is looking at it.
            let path = std::env::temp_dir().join("aloud-screen-example.jpg");
            use base64::Engine as _;
            match base64::engine::general_purpose::STANDARD.decode(b64) {
                Ok(bytes) => {
                    std::fs::write(&path, &bytes).expect("write jpeg");
                    println!("wrote {} bytes to {}", bytes.len(), path.display());
                }
                Err(e) => println!("data URI did not decode: {e}"),
            }
        }
        None if !granted => {
            println!("screenshot: <none — grant Screen Recording in System Settings and rerun>")
        }
        None => println!("screenshot: <none — capture failed with the permission granted>"),
    }
}
