//! Reads the selected text in whatever app has focus, for command mode.
//!
//! macOS has a proper answer — the Accessibility API's `AXSelectedText`,
//! ported from `AppContextService.swift`. Windows UI Automation is unreliable
//! across apps, so both platforms fall back to synthesizing the copy chord and
//! reading the clipboard (via `aloud-paste`), which works anywhere text can be
//! selected.

use anyhow::Result;

/// The current selection, or an empty string when nothing is selected.
///
/// Tries the native path first where there is one, then the clipboard trick.
pub fn selected_text() -> Result<String> {
    #[cfg(target_os = "macos")]
    if let Some(text) = macos::selected_text() {
        return Ok(text);
    }

    // Cross-platform baseline: Cmd/Ctrl+C, read clipboard, restore it.
    aloud_paste::copy_selection()
}

#[cfg(target_os = "macos")]
mod macos {
    //! `AXUIElementCopyAttributeValue(kAXSelectedTextAttribute)` on the focused
    //! element. Needs Accessibility permission, which the app already requires
    //! for hotkeys.

    use core_foundation::base::{CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    type AXError = i32;
    const AX_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn CFRelease(cf: CFTypeRef);
    }

    /// Reads one string attribute off an element, releasing the copy.
    fn copy_string(element: AXUIElementRef, attribute: &str) -> Option<String> {
        let attr = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: `attr` outlives the call; a success result hands us a +1
        // reference we release after reading.
        let err = unsafe {
            AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value)
        };
        if err != AX_SUCCESS || value.is_null() {
            return None;
        }
        let text = unsafe { CFString::wrap_under_get_rule(value as CFStringRef) }.to_string();
        unsafe { CFRelease(value) };
        Some(text)
    }

    pub fn selected_text() -> Option<String> {
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return None;
        }

        // The focused element, then its selection.
        let focused = copy_element(system, "AXFocusedUIElement");
        unsafe { CFRelease(system as CFTypeRef) };
        let focused = focused?;

        let text = copy_string(focused, "AXSelectedText");
        unsafe { CFRelease(focused as CFTypeRef) };

        // An empty selection means "nothing here", so let the caller fall back.
        text.filter(|t| !t.is_empty())
    }

    /// Reads an element-valued attribute (the focused element).
    fn copy_element(element: AXUIElementRef, attribute: &str) -> Option<AXUIElementRef> {
        let attr = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value)
        };
        if err != AX_SUCCESS || value.is_null() {
            return None;
        }
        Some(value as AXUIElementRef)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_text_does_not_panic_with_no_selection() {
        // Both paths must return, not crash, when nothing is selected. In CI
        // there is no focused UI, so this exercises the None/empty branches.
        let _ = super::selected_text();
    }
}
