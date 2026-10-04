//! Reads the selected text in whatever app has focus, so an utterance can
//! rewrite it instead of being typed.
//!
//! macOS uses the Accessibility API's `AXSelectedText`. Windows UI Automation is
//! unreliable across apps, so both fall back to synthesizing the copy chord and
//! reading the clipboard (`aloud-paste`).

use anyhow::Result;

/// The current selection without touching the clipboard, or None when the OS
/// cannot say.
///
/// Accessibility read only — no Cmd+C fallback. This runs on every utterance,
/// and synthesising a copy that often is slow and destructive in any app where
/// Cmd+C does something of its own. None is the safe way to be wrong: the
/// caller falls back to plain dictation.
pub fn selected_text_quietly() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::selected_text().filter(|t| !t.trim().is_empty())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ponytail: Windows has no equivalent read yet, so dictation there is
        // never selection-aware. UI Automation's TextPattern is the upgrade.
        None
    }
}

/// The current selection, or an empty string when nothing is selected.
///
/// Costs a synthesised Cmd+C wherever Accessibility cannot answer, so this is
/// for the explicit rewrite hotkey; see [`selected_text_quietly`] otherwise.
pub fn selected_text() -> Result<String> {
    #[cfg(target_os = "macos")]
    if let Some(text) = macos::selected_text() {
        return Ok(text);
    }

    // Cross-platform baseline: Cmd/Ctrl+C, read clipboard, restore it.
    aloud_paste::copy_selection()
}

/// The app that currently has focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedApp {
    /// Display name, e.g. "Slack".
    pub name: String,
    /// `CFBundleIdentifier`, when the process lives in an `.app`. None for a
    /// plain binary and everywhere off macOS.
    pub bundle_id: Option<String>,
}

/// The app that currently has focus. None means "no app matched", so app rules
/// simply don't fire.
pub fn frontmost_app() -> Option<FocusedApp> {
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

/// The title of the focused window, e.g. "Re: Q3 planning — Mail" — the cheapest
/// signal about what the user is looking at, for the cleanup prompt's context
/// clause. None for the many windows, menus and sheets with no title.
pub fn frontmost_window_title() -> Option<String> {
    frontmost_window()
        .map(|(title, _)| title)
        .filter(|t| !t.trim().is_empty())
}

/// The focused window's title (empty when it has none) and, when it shows a
/// file, that file's path from `AXDocument` — which project the user is in, so
/// "open components" can mean the one in it. None when no window has focus.
pub fn frontmost_window() -> Option<(String, Option<String>)> {
    #[cfg(target_os = "macos")]
    {
        macos::frontmost_window()
    }
    #[cfg(not(target_os = "macos"))]
    {
        // ponytail: Windows returns None, so screen context there is the app
        // name alone. `GetWindowTextW(GetForegroundWindow())` is the upgrade.
        None
    }
}

/// Staged accessors behind the frontmost-app read, for the `frontmost` example:
/// the read fails identically for a permission denial and a bad attribute,
/// which is how it stayed broken.
#[cfg(target_os = "macos")]
pub mod debug {
    pub fn is_trusted() -> bool {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }

    pub fn frontmost_pid() -> Option<i32> {
        super::macos::frontmost_pid()
    }

    pub fn executable_path(pid: i32) -> Option<String> {
        super::macos::executable_path(pid)
    }

    /// The raw AX error code. -25204 is `kAXErrorCannotComplete`, which is what
    /// a locked screen looks like — not a permission problem.
    pub fn frontmost_error() -> i32 {
        super::macos::frontmost_error()
    }
}

#[cfg(not(target_os = "macos"))]
pub mod debug {
    pub fn is_trusted() -> bool {
        true
    }
    pub fn frontmost_pid() -> Option<i32> {
        None
    }
    pub fn executable_path(_pid: i32) -> Option<String> {
        None
    }
    pub fn frontmost_error() -> i32 {
        0
    }
}

/// An installed application, for the App Rules picker.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct InstalledApp {
    /// Display name, e.g. "Slack".
    pub name: String,
    /// `CFBundleIdentifier`, so a rule created from the picker can be keyed on
    /// something that survives the app being renamed. None when the bundle does
    /// not declare one.
    pub bundle_id: Option<String>,
    /// A `data:image/png;base64,…` URI for the app icon, or None if it could
    /// not be read. Lets the picker show icons without a second round-trip.
    pub icon: Option<String>,
}

/// Every installed app, sorted by name. The names are the same ones
/// `frontmost_app()` reports, so a name picked here matches at dictation time.
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
    //! element. Needs the Accessibility permission hotkeys already require.

    use core_foundation::base::{CFGetTypeID, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::url::{CFURLCreateWithString, CFURL};
    use std::ffi::c_void;

    type AXUIElementRef = *const c_void;
    type AXError = i32;
    const AX_SUCCESS: AXError = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn CFRelease(cf: CFTypeRef);
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
    }

    extern "C" {
        /// libproc, linked into every process on macOS.
        fn proc_pidpath(pid: i32, buf: *mut std::ffi::c_void, size: u32) -> i32;
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
        // An app may answer with a URL or number where a string is documented;
        // reading that as a CFString is undefined behaviour, not a None.
        if unsafe { CFGetTypeID(value) } != CFString::type_id() {
            unsafe { CFRelease(value) };
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

        let focused = copy_element(system, "AXFocusedUIElement");
        unsafe { CFRelease(system as CFTypeRef) };
        let focused = focused?;

        let text = copy_string(focused, "AXSelectedText");
        unsafe { CFRelease(focused as CFTypeRef) };

        // An empty selection means "nothing here", so let the caller fall back.
        text.filter(|t| !t.is_empty())
    }

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

    /// The focused application's display name, from the process's bundle path
    /// and never `AXTitle` — AXTitle on an *application* element is empty for
    /// most apps, so it returned None for everything and no app rule ever fired.
    /// The `.app` stem is also what `installed_apps` puts in the picker.
    pub fn frontmost_app() -> Option<super::FocusedApp> {
        let pid = frontmost_pid()?;
        let path = executable_path(pid)?;
        Some(super::FocusedApp {
            name: bundle_name(&path)?,
            bundle_id: bundle_path(&path).as_deref().and_then(bundle_id),
        })
    }

    /// The focused window's `AXTitle` and `AXDocument`, read off
    /// `AXFocusedWindow` of the focused *application* element: nothing
    /// implements the system-wide `AXFocusedWindow`, so asking for it returns
    /// None for every app.
    pub(super) fn frontmost_window() -> Option<(String, Option<String>)> {
        let pid = frontmost_pid()?;
        // SAFETY: an application element for a live pid; released below.
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        let window = copy_element(app, "AXFocusedWindow");
        unsafe { CFRelease(app as CFTypeRef) };
        let window = window?;

        let title = copy_string(window, "AXTitle").unwrap_or_default();
        let document = copy_string(window, "AXDocument").and_then(|url| file_path(&url));
        unsafe { CFRelease(window as CFTypeRef) };
        Some((title, document))
    }

    /// The path a `file://` URL names, percent-decoded. `AXDocument` is a URL
    /// string, and a caller walking up to a repo root needs the path.
    pub(super) fn file_path(url: &str) -> Option<String> {
        if !url.starts_with("file:") {
            return None;
        }
        let string = CFString::new(url);
        // SAFETY: a null allocator is the default one; a non-null result is a
        // +1 CFURL, wrapped under the create rule so it is released on drop.
        let raw = unsafe {
            CFURLCreateWithString(
                std::ptr::null(),
                string.as_concrete_TypeRef(),
                std::ptr::null(),
            )
        };
        if raw.is_null() {
            return None;
        }
        let url = unsafe { CFURL::wrap_under_create_rule(raw) };
        url.to_path().map(|p| p.display().to_string())
    }

    /// The focused application's process id. Which attribute answers varies with
    /// what has focus — a text field gives both, a menu often only the first.
    pub(super) fn frontmost_pid() -> Option<i32> {
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return None;
        }
        let focused = copy_element(system, "AXFocusedApplication")
            .or_else(|| copy_element(system, "AXFocusedUIElement"));
        unsafe { CFRelease(system as CFTypeRef) };
        let focused = focused?;

        let mut pid: i32 = 0;
        let err = unsafe { AXUIElementGetPid(focused, &mut pid) };
        unsafe { CFRelease(focused as CFTypeRef) };
        (err == AX_SUCCESS && pid > 0).then_some(pid)
    }

    /// The raw AX error, so the `frontmost` example can tell a permission
    /// problem from a locked screen (`kAXErrorCannotComplete`, -25204).
    pub(super) fn frontmost_error() -> i32 {
        let system = unsafe { AXUIElementCreateSystemWide() };
        if system.is_null() {
            return -1;
        }
        let attr = CFString::new("AXFocusedApplication");
        let mut value: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyAttributeValue(system, attr.as_concrete_TypeRef(), &mut value)
        };
        if !value.is_null() {
            unsafe { CFRelease(value) };
        }
        unsafe { CFRelease(system as CFTypeRef) };
        err
    }

    pub(super) fn executable_path(pid: i32) -> Option<String> {
        let mut buf = [0u8; 4096];
        let written = unsafe { proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if written <= 0 {
            return None;
        }
        Some(String::from_utf8_lossy(&buf[..written as usize]).into_owned())
    }

    /// The `.app` bundle name from an executable path, falling back to the file
    /// name for a plain binary. The *outermost* `.app`: Electron nests helper
    /// bundles, and a rule written against "Cursor" has to match when focus is
    /// in `Cursor Helper.app`, which is most of the time.
    pub(super) fn bundle_name(exec_path: &str) -> Option<String> {
        let path = Path::new(exec_path);
        bundle_path(exec_path)
            .as_deref()
            .and_then(|a| a.file_stem())
            .or_else(|| path.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
    }

    /// The outermost `.app` directory containing `exec_path`, or None for a
    /// plain binary. See [`bundle_name`] for why outermost.
    pub(super) fn bundle_path(exec_path: &str) -> Option<PathBuf> {
        // `ancestors()` walks deepest-first, so the outermost match is last.
        Path::new(exec_path)
            .ancestors()
            .filter(|a| a.extension().is_some_and(|e| e == "app"))
            .last()
            .map(Path::to_path_buf)
    }

    /// A bundle's `CFBundleIdentifier`. A string-slice read, not a plist parser;
    /// a failure is survivable, since the caller falls back to name matching.
    ///
    /// ponytail: binary plists read as None. Measured on this machine, that is
    /// 1 app in 71 — Xcode. `plutil -convert xml1 -o -` (or the `plist` crate)
    /// is the upgrade path if that list ever grows.
    pub(super) fn bundle_id(app: &Path) -> Option<String> {
        let plist = std::fs::read_to_string(app.join("Contents/Info.plist")).ok()?;
        plist_string(&plist, "CFBundleIdentifier")
    }

    /// The `<string>` following `<key>{key}</key>` in an XML plist.
    pub(super) fn plist_string(plist: &str, key: &str) -> Option<String> {
        plist
            .split(&format!("<key>{key}</key>"))
            .nth(1)
            .and_then(|s| s.split("<string>").nth(1))
            .and_then(|s| s.split("</string>").next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    use super::InstalledApp;
    use std::path::{Path, PathBuf};

    /// Every `.app` under the standard application folders, sorted by name.
    /// Scanning the folders is the AppKit-free way to enumerate them; icons are
    /// best-effort so a missing one never drops an app.
    ///
    /// Cached for [`CACHE_TTL`] — the walk costs ~0.3s for 70 apps, nearly all
    /// `sips` spawns. A *process-lifetime* cache would be wrong: this app lives
    /// in the tray for weeks, so a newly installed app would never appear.
    pub fn installed_apps() -> Vec<InstalledApp> {
        static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<InstalledApp>)>> =
            std::sync::Mutex::new(None);

        if let Ok(cache) = CACHE.lock() {
            if let Some((scanned_at, apps)) = cache.as_ref() {
                if scanned_at.elapsed() < CACHE_TTL {
                    return apps.clone();
                }
            }
        }

        // Not holding the lock across the scan: two pickers opening at once
        // would rather duplicate 0.3s of work than block each other.
        let apps = scan();
        if let Ok(mut cache) = CACHE.lock() {
            *cache = Some((std::time::Instant::now(), apps.clone()));
        }
        apps
    }

    /// How long a scan stays fresh. See [`installed_apps`].
    pub(super) const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

    fn scan() -> Vec<InstalledApp> {
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

        // `sips` is one process per app; ~100 in parallel beats a serial crawl.
        let mut apps: Vec<InstalledApp> = std::thread::scope(|s| {
            let handles: Vec<_> = bundles
                .iter()
                .map(|(name, path)| {
                    s.spawn(move || InstalledApp {
                        name: name.clone(),
                        bundle_id: bundle_id(path),
                        icon: icon_data_uri(path),
                    })
                })
                .collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    /// A small PNG icon for an `.app` as a data URI, or None. `sips` reads the
    /// multi-resolution `.icns` so we do not have to parse ICNS ourselves.
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
        std::fs::File::open(&out)
            .ok()?
            .read_to_end(&mut bytes)
            .ok()?;
        let _ = std::fs::remove_file(&out);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Some(format!("data:image/png;base64,{b64}"))
    }

    /// The bundle's icon file: `CFBundleIconFile` if the plist names one (it may
    /// lack the extension), otherwise the first `.icns` in Resources.
    fn find_icns(app: &Path) -> Option<PathBuf> {
        let resources = app.join("Contents/Resources");
        if let Ok(plist) = std::fs::read_to_string(app.join("Contents/Info.plist")) {
            if let Some(name) = plist_string(&plist, "CFBundleIconFile") {
                let named = resources.join(&name);
                if named.exists() {
                    return Some(named);
                }
                let with_ext = resources.join(format!("{name}.icns"));
                if with_ext.exists() {
                    return Some(with_ext);
                }
            }
        }
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
        // In CI there is no focused UI, so this exercises the None/empty branch.
        let _ = super::selected_text();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod cache_tests {
    /// Guards: a process-lifetime cache in an app that is never quit means an
    /// app installed today never shows up in App Rules.
    #[test]
    fn cache_ttl_is_short_enough_for_a_long_running_app() {
        assert!(
            super::macos::CACHE_TTL <= std::time::Duration::from_secs(600),
            "Aloud runs for weeks in the tray; a long TTL hides newly installed apps"
        );
    }

    #[test]
    fn second_call_is_served_from_cache() {
        let cold = std::time::Instant::now();
        let first = super::installed_apps();
        let cold = cold.elapsed();

        let warm = std::time::Instant::now();
        let second = super::installed_apps();
        let warm = warm.elapsed();

        assert_eq!(first.len(), second.len());
        if !first.is_empty() {
            assert!(
                warm < cold / 5,
                "cold {cold:?} vs warm {warm:?} — not cached"
            );
        }
    }

    /// Guards: `AXTitle` off the application element is empty for most apps, so
    /// `frontmost_app` returned None for everything and no rule ever fired.
    #[test]
    #[cfg(target_os = "macos")]
    fn a_bundle_path_yields_the_name_a_rule_would_be_written_against() {
        use super::macos::bundle_name;

        assert_eq!(
            bundle_name("/Applications/Slack.app/Contents/MacOS/Slack").as_deref(),
            Some("Slack")
        );
        // Nested helpers resolve to the outermost .app, which is what is listed.
        assert_eq!(
            bundle_name("/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Helper")
                .as_deref(),
            Some("Cursor")
        );
        assert_eq!(bundle_name("/usr/bin/vim").as_deref(), Some("vim"));
        assert_eq!(bundle_name(""), None);
    }

    /// Reading the wrong `<string>` silently mis-keys every rule the picker
    /// writes; the neighbouring keys are what a looser slice would grab.
    #[test]
    fn the_bundle_id_key_is_read_and_not_a_neighbouring_one() {
        use super::macos::plist_string;
        let plist = r#"<plist><dict>
            <key>CFBundleIconFile</key><string>AppIcon</string>
            <key>CFBundleIdentifier</key><string>com.tinyspeck.slackmacgap</string>
            <key>CFBundleName</key><string>Slack</string>
        </dict></plist>"#;

        assert_eq!(
            plist_string(plist, "CFBundleIdentifier").as_deref(),
            Some("com.tinyspeck.slackmacgap")
        );
        assert_eq!(
            plist_string(plist, "CFBundleIconFile").as_deref(),
            Some("AppIcon")
        );
        // An absent key must be None, not the first string in the file.
        assert_eq!(plist_string(plist, "CFBundleVersion"), None);
        // A binary plist must read as None, so the caller falls back to names.
        assert_eq!(
            plist_string("bplist00\u{0}\u{1}", "CFBundleIdentifier"),
            None
        );
    }

    /// TextEdit reports `file:///…/ax%20test/probe%20file.txt`; walking that up
    /// to a repo root needs the decoded path, and a web page is no document.
    #[test]
    fn an_ax_document_url_becomes_a_path() {
        use super::macos::file_path;
        assert_eq!(
            file_path("file:///tmp/ax%20test/probe%20file.txt").as_deref(),
            Some("/tmp/ax test/probe file.txt")
        );
        assert_eq!(file_path("https://example.com/a"), None);
    }

    /// Outermost, for the same reason the name is: an Electron helper declares a
    /// different id from the app the user picked.
    #[test]
    fn the_bundle_path_is_the_outermost_app_not_the_helper() {
        use super::macos::bundle_path;
        assert_eq!(
            bundle_path(
                "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper.app/Contents/MacOS/Helper"
            )
            .as_deref(),
            Some(std::path::Path::new("/Applications/Cursor.app"))
        );
        assert_eq!(bundle_path("/usr/bin/vim"), None);
    }
}
