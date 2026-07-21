//! What macOS is currently letting this app do, and how to go grant the rest.
//!
//! Every check here is a *preflight*: it reads the existing TCC answer and
//! never prompts. A prompt is the OS's to raise at the moment the capability
//! is first used (the mic sheet on the first dictation, the accessibility
//! sheet from the hotkey tap), and a settings pane that fired three of them on
//! open would be worse than the banner it replaces.
//!
//! **Accessibility is one row, not two.** `AXIsProcessTrusted` is what the
//! event tap, the synthetic paste and the selection reader all require, and
//! `aloud_hotkey::is_permitted` already asks it — so the "hotkeys" permission
//! and the "accessibility" permission are the same TCC bucket and the same
//! call. Input Monitoring is a genuinely different bucket, but nothing here
//! needs it: a listen-only `CGEventTap` would, and ours swallows events, which
//! puts it under Accessibility.
//!
//! Everything is empty off macOS. Windows has no equivalent of TCC for any of
//! this — the mic is a per-app privacy setting with no query API worth the
//! plumbing, and there is no accessibility gate at all.

use serde::{Deserialize, Serialize};

/// A capability the app needs the OS to allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Global hotkeys, synthetic paste, and reading the focused selection.
    Accessibility,
    Microphone,
    /// Screen context. Nothing reads it yet — see ROADMAP Phase 9.
    ScreenRecording,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub permission: Permission,
    pub granted: bool,
}

/// Every permission the app cares about, with its current answer. Empty off
/// macOS, which the UI reads as "no card to show".
pub fn all() -> Vec<PermissionStatus> {
    #[cfg(not(target_os = "macos"))]
    return Vec::new();

    #[cfg(target_os = "macos")]
    [
        Permission::Accessibility,
        Permission::Microphone,
        Permission::ScreenRecording,
    ]
    .into_iter()
    .map(|permission| PermissionStatus {
        permission,
        granted: is_granted(permission),
    })
    .collect()
}

/// Whether `permission` is granted right now, without prompting.
///
/// Public so a capability can preflight itself rather than discovering the
/// denial as a confusing success: `screencapture` with Screen Recording
/// denied still writes a PNG, it is just a picture of the wallpaper.
pub fn is_granted(permission: Permission) -> bool {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = permission;
        true
    }
    #[cfg(target_os = "macos")]
    {
        granted(permission)
    }
}

/// Opens the System Settings pane that grants `permission`.
///
/// Shelled out rather than linked: the `x-apple.systempreferences:` scheme is
/// the only documented way in, and `open` is how you hand a URL to it.
pub fn open_settings(permission: Permission) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = permission;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let anchor = match permission {
            Permission::Accessibility => "Privacy_Accessibility",
            Permission::Microphone => "Privacy_Microphone",
            Permission::ScreenRecording => "Privacy_ScreenCapture",
        };
        // `spawn`, not `status`: this reaches Tauri as a non-async command, so
        // it runs on the main thread, and waiting for LaunchServices to open
        // System Settings freezes the window for as long as that takes — a
        // cold launch is very visible. The exit status was discarded anyway.
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{anchor}"
            ))
            .spawn()?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn granted(permission: Permission) -> bool {
    match permission {
        Permission::Accessibility => aloud_hotkey::is_permitted(),
        Permission::Microphone => macos::microphone_authorized(),
        Permission::ScreenRecording => macos::screen_capture_authorized(),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void};

    /// `AVAuthorizationStatusAuthorized`. The other three (not determined,
    /// restricted, denied) all mean the mic will not open right now, which is
    /// the only distinction the card draws.
    const AUTHORIZED: isize = 3;

    type Id = *mut c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        /// Reads the screen-recording answer without prompting. The prompting
        /// variant is `CGRequestScreenCaptureAccess`.
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: Id;
    }

    // Named explicitly. libobjc happens to arrive as a re-export of the
    // frameworks other crates link, so leaving it off links today and breaks
    // the day this crate is built without them.
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Id;
        fn objc_msgSend();
    }

    pub fn screen_capture_authorized() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    /// `+[AVCaptureDevice authorizationStatusForMediaType:]`.
    ///
    /// Raw runtime calls rather than an `objc2` dependency: this is one class
    /// method with a known return type, and `crates/hotkey` already reaches
    /// for `AXIsProcessTrusted` the same way. `objc_msgSend` is declared
    /// argument-less and transmuted to the real signature because its C
    /// declaration is variadic, and a variadic call would pass these arguments
    /// in the wrong registers on aarch64.
    pub fn microphone_authorized() -> bool {
        unsafe {
            let class = objc_getClass(c"AVCaptureDevice".as_ptr());
            if class.is_null() {
                return false;
            }
            let selector = sel_registerName(c"authorizationStatusForMediaType:".as_ptr());
            let send: extern "C" fn(Id, Id, Id) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            send(class, selector, AVMediaTypeAudio) == AUTHORIZED
        }
    }
}
