//! Windows backend: a WH_KEYBOARD_LL low-level hook.
//!
//! The equivalent of the macOS event tap, and needed for the same reasons:
//! `RegisterHotKey` cannot see modifier-only holds and cannot swallow events.
//! The hook callback is a C function pointer with no user data, so the handler
//! lives in a thread-local set up by `run_hook`.

use crate::model::Key;
use anyhow::{anyhow, Result};
use std::cell::RefCell;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

pub struct RawEvent {
    pub key: Key,
    pub down: bool,
}

type Handler = Box<dyn FnMut(RawEvent) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

/// Maps a virtual-key code plus the extended-key flag to a `Key`.
///
/// Windows reports left/right modifiers as distinct VKs (VK_LMENU/VK_RMENU),
/// so the side-specific bindings the Swift app supported carry over. `Fn` is
/// absent: it is handled in keyboard firmware and never reaches the hook.
pub fn key_from_vk(vk: u16, extended: bool) -> Option<Key> {
    let vk = VIRTUAL_KEY(vk);
    Some(match vk {
        VK_LWIN => Key::LeftCommand,
        VK_RWIN => Key::RightCommand,
        VK_LSHIFT => Key::LeftShift,
        VK_RSHIFT => Key::RightShift,
        VK_LMENU => Key::LeftOption,
        VK_RMENU => Key::RightOption,
        VK_LCONTROL => Key::LeftControl,
        VK_RCONTROL => Key::RightControl,
        // Generic modifier VKs: the extended-key flag marks the right side.
        VK_SHIFT => {
            if extended {
                Key::RightShift
            } else {
                Key::LeftShift
            }
        }
        VK_CONTROL => {
            if extended {
                Key::RightControl
            } else {
                Key::LeftControl
            }
        }
        VK_MENU => {
            if extended {
                Key::RightOption
            } else {
                Key::LeftOption
            }
        }
        VK_ESCAPE => Key::Escape,
        VK_SPACE => Key::Space,
        _ => {
            let code = vk.0;
            match code {
                0x41..=0x5A => Key::Char((code as u8 as char).to_ascii_lowercase()),
                0x70..=0x7B => Key::F((code - 0x6F) as u8), // VK_F1..VK_F12
                _ => return None,
            }
        }
    })
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let is_key_event = down || matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);

    let swallow = if is_key_event {
        const LLKHF_EXTENDED: u32 = 0x01;
        let extended = info.flags.0 & LLKHF_EXTENDED != 0;
        match key_from_vk(info.vkCode as u16, extended) {
            Some(key) => HANDLER.with(|h| {
                h.borrow_mut()
                    .as_mut()
                    .is_some_and(|handler| handler(RawEvent { key, down }))
            }),
            None => false,
        }
    } else {
        false
    };

    // A non-zero return swallows the event.
    if swallow {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

/// Installs the hook and pumps messages on the current thread. Blocks forever.
///
/// A low-level hook requires a message loop on the installing thread — without
/// one Windows silently stops delivering events.
pub fn run_hook<F>(on_event: F) -> Result<()>
where
    F: FnMut(RawEvent) -> bool + 'static,
{
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(on_event)));

    let hook: HHOOK = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
        .map_err(|e| anyhow!("failed to install keyboard hook: {e}"))?;

    tracing::info!("keyboard hook running");

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
    Ok(())
}

/// Windows has no Accessibility gate for keyboard hooks.
pub fn is_trusted() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_side_specific_modifiers() {
        assert_eq!(key_from_vk(VK_RMENU.0, false), Some(Key::RightOption));
        assert_eq!(key_from_vk(VK_LMENU.0, false), Some(Key::LeftOption));
        assert_eq!(key_from_vk(VK_ESCAPE.0, false), Some(Key::Escape));
    }

    /// Generic modifier VKs disambiguate by the extended-key flag.
    #[test]
    fn extended_flag_selects_right_side() {
        assert_eq!(key_from_vk(VK_CONTROL.0, true), Some(Key::RightControl));
        assert_eq!(key_from_vk(VK_CONTROL.0, false), Some(Key::LeftControl));
    }

    #[test]
    fn maps_letters_and_function_keys() {
        assert_eq!(key_from_vk(0x44, false), Some(Key::Char('d')));
        assert_eq!(key_from_vk(VK_F5.0, false), Some(Key::F(5)));
        assert_eq!(key_from_vk(0x00, false), None);
    }
}
