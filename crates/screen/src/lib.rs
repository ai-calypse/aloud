//! What the user is looking at, filling in the context clause in
//! `prompts/cleanup.md`.
//!
//! Two halves: a **text summary** (frontmost app and window title — free, and
//! covered by the Accessibility grant hotkeys already need), and a
//! **screenshot** as a JPEG data URI, which needs the Screen Recording grant
//! and is off by default — see [`Context::screenshot`].
//!
//! Everything returns None rather than failing: a denial, a locked screen and an
//! untitled window all mean the same thing, dictate without context.

/// Longest edge of the captured screenshot. A 5K display is tens of megabytes,
/// which base64 grows by a third; 1024 is still legible enough to read a mail
/// header off.
pub const DEFAULT_MAX_DIMENSION: u32 = 1024;

/// A snapshot of what has focus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// One line naming the app and window. Empty means send no context at all,
    /// rather than an "unknown" the model treats as a fact about the screen.
    pub summary: String,
    /// `data:image/jpeg;base64,…`, or None when not requested, not permitted,
    /// or not captured.
    ///
    /// ponytail: no consumer in the dictation path — cleanup is a text-only
    /// call. The upgrade path is a synthesis step turning this image into
    /// `summary` before cleanup runs.
    pub screenshot: Option<String>,
}

/// Reads the current context. `with_screenshot` costs a `screencapture`
/// subprocess (~200 ms), so it is gated on a setting that defaults to off.
pub fn capture(with_screenshot: bool) -> Context {
    Context {
        summary: summary(),
        screenshot: with_screenshot
            .then(|| screenshot_data_uri(DEFAULT_MAX_DIMENSION))
            .flatten(),
    }
}

/// The frontmost app and window title as one line, or empty.
pub fn summary() -> String {
    compose(
        aloud_selection::frontmost_app().map(|a| a.name).as_deref(),
        aloud_selection::frontmost_window_title().as_deref(),
    )
}

/// Builds the summary line, separated from the OS reads because this is the half
/// that can be silently wrong and the prompt treats it as ground truth.
fn compose(app: Option<&str>, title: Option<&str>) -> String {
    let app = app.map(str::trim).filter(|s| !s.is_empty());
    let title = title.map(str::trim).filter(|s| !s.is_empty());
    match (app, title) {
        // Apps put their own name in the title bar ("Inbox — Mail"); repeating
        // it reads as two different facts about the screen.
        (Some(app), Some(title)) if title.contains(app) => title.to_string(),
        (Some(app), Some(title)) => format!("{app} — {title}"),
        (Some(app), None) => app.to_string(),
        (None, Some(title)) => title.to_string(),
        (None, None) => String::new(),
    }
}

/// The whole screen as a JPEG data URI, downscaled so its longest edge is at
/// most `max_dimension`. None if the permission is missing or capture failed.
pub fn screenshot_data_uri(max_dimension: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::screenshot_data_uri(max_dimension)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ponytail: Windows returns None, so screen context there is the app
        // name and window title only. `BitBlt` over the virtual desktop, or
        // `Windows.Graphics.Capture`, is the upgrade path.
        let _ = max_dimension;
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use base64::Engine;

    /// ponytail: shells out to `screencapture` rather than binding
    /// ScreenCaptureKit — same TCC bucket, twenty lines instead of hundreds. The
    /// ceiling is ~200ms and no way to capture one window or exclude our own
    /// overlay; `SCScreenshotManager.captureImage` is the upgrade path.
    pub fn screenshot_data_uri(max_dimension: u32) -> Option<String> {
        // Preflight: with Screen Recording denied, `screencapture` still exits 0
        // and still writes a PNG — of the wallpaper, every window missing.
        if !aloud_permissions::is_granted(aloud_permissions::Permission::ScreenRecording) {
            tracing::debug!("screen recording not granted; no screenshot");
            return None;
        }

        // Unique per capture, not per process: overlapping dictations on a
        // shared path means one run's `screencapture` overwrites what the
        // other's `sips` is reading.
        let out = std::env::temp_dir().join(format!(
            "aloud-screen-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        // -x: no shutter sound. -t png: lossless going in, so the only quality
        // loss is the downscale below.
        let captured = std::process::Command::new("screencapture")
            .args(["-x", "-t", "png"])
            .arg(&out)
            .output()
            .ok()
            .is_some_and(|o| o.status.success());
        if !captured {
            tracing::warn!("screencapture failed");
            // It may have written the file before failing, and a full-screen
            // PNG is not something to leave in temp.
            let _ = std::fs::remove_file(&out);
            return None;
        }

        // In-place, so a failure leaves a valid but unsendably large PNG —
        // a warning, not a bail. JPEG is not a detail: a 1024-wide PNG of a
        // plain desktop measured 1.0 MB here, 1.3M characters once base64'd,
        // larger than most context windows before the transcript is added.
        let resized = std::process::Command::new("sips")
            .args([
                "-Z",
                &max_dimension.to_string(),
                "-s",
                "format",
                "jpeg",
                "-s",
                "formatOptions",
                "50",
            ])
            .arg(&out)
            .output()
            .ok()
            .is_some_and(|o| o.status.success());
        if !resized {
            tracing::warn!("sips could not downscale the screenshot");
        }

        let bytes = std::fs::read(&out).ok();
        let _ = std::fs::remove_file(&out);
        let bytes = bytes?;
        // An empty or absurdly small file is a capture that did not happen.
        if bytes.len() < 1024 {
            tracing::warn!(bytes = bytes.len(), "screenshot too small to be real");
            return None;
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        // The .png extension is a lie after the sips re-encode; the MIME type
        // is what the model reads and must match the bytes.
        Some(format!("data:image/jpeg;base64,{b64}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{capture, compose};

    /// Guards: a summary of "unknown" or "— " reads to the model as a fact
    /// about the screen.
    #[test]
    fn nothing_known_yields_no_context_rather_than_a_placeholder() {
        assert_eq!(compose(None, None), "");
        assert_eq!(compose(Some("  "), Some("")), "");
        assert_eq!(compose(None, Some("Inbox")), "Inbox");
        assert_eq!(compose(Some("Mail"), None), "Mail");
    }

    /// "Mail — Inbox — Mail" spends tokens on a duplicate and reads as two
    /// windows.
    #[test]
    fn an_app_name_already_in_the_title_is_not_repeated() {
        assert_eq!(compose(Some("Mail"), Some("Inbox — Mail")), "Inbox — Mail");
        assert_eq!(
            compose(Some("Slack"), Some("general (Aloud)")),
            "Slack — general (Aloud)"
        );
    }

    /// Deliberately does not call `screenshot_data_uri`: where Screen Recording
    /// is granted that would photograph whoever ran `cargo test`. The `status`
    /// example covers it, opt-in.
    #[test]
    fn asking_for_no_screenshot_takes_none() {
        assert!(capture(false).screenshot.is_none());
    }
}
