//! What the user is looking at, in a form the cleanup prompt can use.
//!
//! `prompts/cleanup.md` has always carried a context clause — "if the context
//! clearly shows email recipients or participants, use those visible names as
//! a strong spelling reference" — ported verbatim from Swift's
//! `PostProcessingService`. Nothing ever filled it in. This crate is what
//! fills it in.
//!
//! Two halves, deliberately unequal:
//!
//! * A **text summary** — the frontmost app and its window title. Free, needs
//!   no permission beyond the Accessibility one hotkeys already require, and
//!   is exactly what that clause asks for: a mail window's title carries the
//!   subject and usually the correspondent.
//! * A **screenshot**, as a JPEG data URI. Needs the Screen Recording TCC
//!   grant, which Aloud has never asked for, so it is off by default and
//!   nothing in the dictation path consumes the image yet — see
//!   [`Context::screenshot`].
//!
//! Everything here returns None rather than failing. A denied permission, a
//! locked screen and an app with no titled window all look the same to the
//! caller, and all mean the same thing: dictate without context, which is what
//! every dictation did before this crate existed.

/// Longest edge of the captured screenshot, in pixels.
///
/// A 5K display is tens of megabytes, which base64 grows by a third — an
/// unsendable data URI and a very slow one to build. 1024 is what the Swift
/// app settled on (`AppContextService.defaultScreenshotMaxDimension`) and is
/// still legible enough to read a mail header off.
pub const DEFAULT_MAX_DIMENSION: u32 = 1024;

/// A snapshot of what has focus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// One line naming the app and window, or empty when nothing is known.
    /// Empty is the signal to send no context at all rather than to send the
    /// word "unknown", which the model would treat as a fact about the screen.
    pub summary: String,
    /// `data:image/jpeg;base64,…`, or None when not requested, not permitted,
    /// or not captured.
    ///
    /// ponytail: no consumer in the dictation path. Cleanup is a text-only
    /// call, and feeding an image to it means a vision model, a second prompt
    /// and a second round-trip in front of every paste — Swift's design, and a
    /// much larger decision than this crate. The upgrade path is a context
    /// synthesis step (Swift's `AppContextService.currentActivity`) that turns
    /// this image into `summary` before cleanup runs.
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

/// The frontmost app and window title as one line, or empty when neither can
/// be read.
pub fn summary() -> String {
    compose(
        aloud_selection::frontmost_app().map(|a| a.name).as_deref(),
        aloud_selection::frontmost_window_title().as_deref(),
    )
}

/// Builds the summary line. Separated from the OS reads because this is the
/// half that can be silently wrong — a summary that says the wrong thing is
/// worse than no summary, since the prompt treats it as ground truth.
fn compose(app: Option<&str>, title: Option<&str>) -> String {
    let app = app.map(str::trim).filter(|s| !s.is_empty());
    let title = title.map(str::trim).filter(|s| !s.is_empty());
    match (app, title) {
        // Plenty of apps put their own name in the title bar ("Inbox — Mail").
        // Repeating it reads as two different facts about the screen.
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
    /// ScreenCaptureKit. It hits the same TCC bucket, needs no framework
    /// bindings and is twenty lines instead of several hundred — the ceiling
    /// is that it costs a process spawn plus two file writes (~200 ms) and
    /// cannot capture a single window or exclude our own overlay.
    /// `SCScreenshotManager.captureImage` is the upgrade path when either of
    /// those starts to matter.
    pub fn screenshot_data_uri(max_dimension: u32) -> Option<String> {
        // Preflight, because a denial here is not an error: `screencapture`
        // with Screen Recording denied still exits 0 and still writes a PNG —
        // of the desktop wallpaper, with every window missing. Handing that to
        // the model as "what the user is looking at" is worse than no context.
        if !aloud_permissions::is_granted(aloud_permissions::Permission::ScreenRecording) {
            tracing::debug!("screen recording not granted; no screenshot");
            return None;
        }

        let out = std::env::temp_dir().join(format!("aloud-screen-{}.png", std::process::id()));
        // -x: no shutter sound. -t png: sips reads it and it is lossless
        //  going in, so the only quality loss is the downscale below.
        let captured = std::process::Command::new("screencapture")
            .args(["-x", "-t", "png"])
            .arg(&out)
            .output()
            .ok()
            .is_some_and(|o| o.status.success());
        if !captured {
            tracing::warn!("screencapture failed");
            return None;
        }

        // Same tool `aloud-selection` resizes app icons with. In-place, so a
        // failure leaves the full-resolution PNG — still a valid capture, just
        // an unsendably large one, hence a warning rather than a bail.
        //
        // JPEG, not PNG, and that is not a detail: measured here, a 1024-wide
        // PNG of a plain desktop is 1.0 MB, which base64 inflates to 1.3M
        // characters — larger than most context windows before the transcript
        // is even added. Quality 50 is what Swift's `AppContextService` uses
        // and is still legible enough to read a mail header off.
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
        // The extension is a lie after the sips re-encode; the MIME type is
        // what a model reads, and it has to match the bytes.
        Some(format!("data:image/jpeg;base64,{b64}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{capture, compose, screenshot_data_uri};

    /// The failure this guards: a summary of "unknown" or "— " reads to the
    /// model as a fact about the screen. Nothing known must mean nothing sent.
    #[test]
    fn nothing_known_yields_no_context_rather_than_a_placeholder() {
        assert_eq!(compose(None, None), "");
        assert_eq!(compose(Some("  "), Some("")), "");
        assert_eq!(compose(None, Some("Inbox")), "Inbox");
        assert_eq!(compose(Some("Mail"), None), "Mail");
    }

    /// Most macOS title bars already end in the app name. Emitting
    /// "Mail — Inbox — Mail" spends context tokens on a duplicate and reads as
    /// two windows.
    #[test]
    fn an_app_name_already_in_the_title_is_not_repeated() {
        assert_eq!(compose(Some("Mail"), Some("Inbox — Mail")), "Inbox — Mail");
        assert_eq!(
            compose(Some("Slack"), Some("general (Aloud)")),
            "Slack — general (Aloud)"
        );
    }

    /// Denial must be a None, not a panic and not a wallpaper photograph — the
    /// app has never requested this grant, so denied is the common case.
    #[test]
    fn a_missing_permission_is_a_none_not_a_panic() {
        let _ = screenshot_data_uri(256);
        assert!(capture(false).screenshot.is_none());
    }
}
