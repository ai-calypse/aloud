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
    /// Control of Calendar.app (Automation): today's events in the notch.
    Calendar,
    /// Control of Reminders.app (Automation): today's reminders in the notch.
    Reminders,
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
        Permission::Calendar,
        Permission::Reminders,
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
            // Automation grants are per target app, in one pane. Asking first
            // is what makes the prompt appear at all.
            Permission::Calendar | Permission::Reminders => {
                automation::request(permission);
                return Ok(());
            }
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
        Permission::Calendar | Permission::Reminders => automation::granted(permission),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void};

    /// `AVAuthorizationStatusAuthorized`. The other three all mean the mic will
    /// not open, which is the only distinction the card draws.
    const AUTHORIZED: isize = 3;

    type Id = *mut c_void;

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

/// Automation (Apple Events) permission for Calendar.app and Reminders.app.
///
/// macOS can only say whether an app may be controlled while that app is
/// running (`procNotFound` otherwise), and asking must not launch it every time
/// Settings gains focus. So the last answer seen is remembered for the
/// process, and a row with no answer yet reads "not granted" until its
/// button — which does launch the app, hidden — gets one.
#[cfg(target_os = "macos")]
mod automation {
    use super::Permission;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU8, Ordering};

    const NO_ERR: i32 = 0;
    const PROC_NOT_FOUND: i32 = -600;
    const NOT_PERMITTED: i32 = -1743;

    // 0 unknown, 1 granted, 2 refused: one slot per permission.
    static SEEN: [AtomicU8; 2] = [AtomicU8::new(0), AtomicU8::new(0)];

    /// `AEDesc`: a descriptor type and a handle, 16 bytes on 64-bit.
    #[repr(C)]
    struct AeDesc {
        descriptor_type: u32,
        data_handle: *mut c_void,
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn AECreateDesc(
            type_code: u32,
            data: *const c_void,
            size: isize,
            result: *mut AeDesc,
        ) -> i16;
        fn AEDisposeDesc(desc: *mut AeDesc) -> i16;
        fn AEDeterminePermissionToAutomateTarget(
            target: *const AeDesc,
            class: u32,
            id: u32,
            ask_user_if_needed: u8,
        ) -> i32;
    }

    fn four(code: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*code)
    }

    fn target(permission: Permission) -> (&'static str, &'static str, usize) {
        match permission {
            Permission::Reminders => ("com.apple.reminders", "Reminders", 1),
            _ => ("com.apple.iCal", "Calendar", 0),
        }
    }

    /// The OS's answer for `bundle`, optionally raising the consent prompt.
    fn determine(bundle: &str, ask: bool) -> i32 {
        let mut desc = AeDesc {
            descriptor_type: 0,
            data_handle: std::ptr::null_mut(),
        };
        // SAFETY: `bundle` outlives the call, `desc` is written by AECreateDesc
        // before it is read, and it is disposed exactly once, on the success path.
        unsafe {
            let made = AECreateDesc(
                four(b"bund"),
                bundle.as_ptr().cast(),
                bundle.len() as isize,
                &mut desc,
            );
            if made != 0 {
                return i32::from(made);
            }
            let status = AEDeterminePermissionToAutomateTarget(
                &desc,
                four(b"****"),
                four(b"****"),
                u8::from(ask),
            );
            AEDisposeDesc(&mut desc);
            status
        }
    }

    fn remember(slot: usize, status: i32) {
        match status {
            NO_ERR => SEEN[slot].store(1, Ordering::Relaxed),
            PROC_NOT_FOUND => {}
            _ => SEEN[slot].store(2, Ordering::Relaxed),
        }
    }

    pub fn granted(permission: Permission) -> bool {
        let (bundle, _, slot) = target(permission);
        remember(slot, determine(bundle, false));
        SEEN[slot].load(Ordering::Relaxed) == 1
    }

    /// Starts the app if it is not open (hidden, never frontmost), asks, and
    /// when the answer is a refusal sends the user to the Automation pane,
    /// where a refusal can be undone. Off the caller's thread: the prompt blocks
    /// until it is answered.
    pub fn request(permission: Permission) {
        std::thread::spawn(move || {
            let (bundle, name, slot) = target(permission);
            let running = |n: &str| {
                std::process::Command::new("pgrep")
                    .args(["-x", n])
                    .stdout(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
            };
            if !running(name) {
                let _ = std::process::Command::new("open")
                    .args(["-g", "-j", "-a", name])
                    .status();
                for _ in 0..30 {
                    if running(name) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
            let status = determine(bundle, true);
            remember(slot, status);
            if status == NOT_PERMITTED {
                let _ = std::process::Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
                    .spawn();
            }
        });
    }
}
