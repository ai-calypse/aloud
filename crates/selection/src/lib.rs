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

/// The name of the app that currently has focus, e.g. "Slack", "Code".
///
/// Used by app rules to vary behaviour per app. Returns None when it can't be
/// determined (no focus, permission denied, or the Windows path, which is not
/// yet implemented). A None means "no app matched", so rules simply don't fire.
pub fn frontmost_app() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_app()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ponytail: Windows returns None, so app rules are a no-op there until
        // GetForegroundWindow + QueryFullProcessImageName is wired up.
        None
    }
}

/// An installed application, for the App Rules picker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct InstalledApp {
    /// Display name, e.g. "Slack". This is what app rules match against.
    pub name: String,
    /// A `data:image/png;base64,…` URI for the app icon, or None if it could
    /// not be read. Lets the picker show icons without a second round-trip.
    pub icon: Option<String>,
}

/// Every installed app, sorted by name. Fills the App Rules picker's search.
///
/// The names are the same display names `frontmost_app()` reports, so a name
/// picked here matches at dictation time. Empty if the list can't be read.
pub fn installed_apps() -> Vec<InstalledApp> {
    #[cfg(target_os = "macos")]
    {
        macos::installed_apps()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ponytail: Windows scans Start Menu .lnk shortcuts — TODO. Until then
        // the picker's search is empty there; the custom-match field still works.
        Vec::new()
    }
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

    /// The focused application's title, via the same system-wide AX element the
    /// selection read uses. No new dependency: `AXFocusedApplication` →
    /// `AXTitle` is the app's display name ("Slack", "Code").
    pub fn frontmost_app() -> Option<String> {
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return None;
        }
        let app = copy_element(system, "AXFocusedApplication");
        unsafe { CFRelease(system as CFTypeRef) };
        let app = app?;

        let title = copy_string(app, "AXTitle");
        unsafe { CFRelease(app as CFTypeRef) };
        title.filter(|t| !t.is_empty())
    }

    use super::InstalledApp;
    use std::path::{Path, PathBuf};

    /// Every `.app` under the standard application folders, sorted by name.
    ///
    /// Scanning the folders is the AppKit-free way to enumerate installed apps:
    /// each `.app` bundle's folder name (minus the extension) is its display
    /// name. Icons are extracted best-effort so a slow or missing icon never
    /// drops an app from the list.
    pub fn installed_apps() -> Vec<InstalledApp> {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Applications/Utilities"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Applications/Utilities"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(Path::new(&home).join("Applications"));
        }

        // Collect (name, bundle path) first, de-duped by name.
        let mut bundles: Vec<(String, PathBuf)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("app") {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if seen.insert(name.to_string()) {
                    bundles.push((name.to_string(), path));
                }
            }
        }

        // Icon extraction shells out to `sips` once per app; doing ~100 of them
        // in parallel keeps the picker snappy instead of a slow serial crawl.
        let mut apps: Vec<InstalledApp> = std::thread::scope(|s| {
            let handles: Vec<_> = bundles
                .iter()
                .map(|(name, path)| s.spawn(move || InstalledApp {
                    name: name.clone(),
                    icon: icon_data_uri(path),
                }))
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    /// Extracts a small PNG icon for an `.app` as a data URI, or None.
    ///
    /// `sips` is the system image tool — it reads the multi-resolution `.icns`
    /// and writes a fixed-size PNG, so we avoid parsing ICNS ourselves. Cheap
    /// enough run once when the picker opens; a failure just yields no icon.
    fn icon_data_uri(app: &Path) -> Option<String> {
        use base64::Engine;
        use std::io::Read;

        let icns = find_icns(app)?;
        // sips can only write to a file, so use a temp path unique to this app.
        let out = std::env::temp_dir().join(format!(
            "aloud-icon-{}.png",
            app.file_stem()?.to_str()?.replace(['/', ' '], "_")
        ));
        let ok = std::process::Command::new("sips")
            .args(["-s", "format", "png", "-Z", "48"])
            .arg(&icns)
            .arg("--out")
            .arg(&out)
            .output()
            .ok()
            .is_some_and(|o| o.status.success());
        if !ok {
            return None;
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&out).ok()?.read_to_end(&mut bytes).ok()?;
        let _ = std::fs::remove_file(&out);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Some(format!("data:image/png;base64,{b64}"))
    }

    /// The bundle's icon file: `CFBundleIconFile` from Info.plist if set,
    /// otherwise the first `.icns` in Resources. Falls back gracefully because
    /// plist parsing is not worth a dependency here.
    fn find_icns(app: &Path) -> Option<PathBuf> {
        let resources = app.join("Contents/Resources");
        // The declared icon, if the plist names one (may lack the extension).
        if let Ok(plist) = std::fs::read_to_string(app.join("Contents/Info.plist")) {
            if let Some(name) = plist
                .split("<key>CFBundleIconFile</key>")
                .nth(1)
                .and_then(|s| s.split("<string>").nth(1))
                .and_then(|s| s.split("</string>").next())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let named = resources.join(name);
                if named.exists() {
                    return Some(named);
                }
                let with_ext = resources.join(format!("{name}.icns"));
                if with_ext.exists() {
                    return Some(with_ext);
                }
            }
        }
        // Otherwise the first .icns we find.
        std::fs::read_dir(&resources).ok()?.flatten().find_map(|e| {
            let p = e.path();
            (p.extension().and_then(|x| x.to_str()) == Some("icns")).then_some(p)
        })
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
