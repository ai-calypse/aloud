//! What macOS is currently letting this app do, and how to go grant the rest.
//!
//! Every check is a *preflight*: it reads the existing TCC answer and never
//! prompts — a settings pane that fired three sheets on open would be worse
//! than the banner it replaces.
//!
//! **Accessibility is one row, not two**: the event tap, the synthetic paste and
//! the selection reader are all the same `AXIsProcessTrusted` bucket. Input
//! Monitoring is a different bucket, but only a listen-only `CGEventTap` needs
//! it, and ours swallows events.
//!
//! Empty off macOS: Windows has no TCC equivalent for any of this.

use serde::{Deserialize, Serialize};

/// A capability the app needs the OS to allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Global hotkeys, synthetic paste, and reading the focused selection.
    Accessibility,
    Microphone,
    ScreenRecording,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub permission: Permission,
    pub granted: bool,
}

/// Every permission the app cares about. Empty off macOS, which the UI reads as
/// "no card to show".
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

/// Whether `permission` is granted right now, without prompting. Public so a
/// capability can preflight rather than meet the denial as a confusing success —
/// `screencapture` denied still writes a PNG, just of the wallpaper.
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

/// Opens the System Settings pane that grants `permission`. Shelled out because
/// `x-apple.systempreferences:` is the only documented way in.
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
        // `spawn`, not `status`: this is a non-async Tauri command on the main
        // thread, and waiting for LaunchServices freezes the window.
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

    /// `AVAuthorizationStatusAuthorized`. The other three all mean the mic will
    /// not open, which is the only distinction the card draws.
    const AUTHORIZED: isize = 3;

    type Id = *mut c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        /// Non-prompting; `CGRequestScreenCaptureAccess` is the prompting one.
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: Id;
    }

    // Named explicitly: libobjc only arrives as a re-export of frameworks other
    // crates link, so omitting it links today and breaks without them.
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Id;
        fn objc_msgSend();
    }

    pub fn screen_capture_authorized() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    /// `+[AVCaptureDevice authorizationStatusForMediaType:]`, via raw runtime
    /// calls rather than an `objc2` dependency. `objc_msgSend` is declared
    /// argument-less and transmuted: its real C declaration is variadic, and a
    /// variadic call passes these arguments in the wrong registers on aarch64.
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
