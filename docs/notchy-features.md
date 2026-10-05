# Notchy's features, and how Aloud would build each one

All 74 features from [notchy.dev](https://notchy.dev/#features) (read 4 October 2026), plus GitHub PRs, which the site lists in a group but not in the feature list. Each one has a high-level plan in Aloud's terms: which crate does the work, which macOS API or CLI it uses, and how the notch shows it.

Most of them are small. A Mac app or CLI already does the hard part (`sips`, `ditto`, `shortcuts`, `caffeinate`, `screencapture`, AppleScript), or one polled system value feeds a small island. The handful that need private APIs or entitlements are marked **L** and flagged.

**Effort:**
- **S**: hours. A CLI call or one API, plus an island.
- **M**: a day or two. A watcher or event tap, a new panel tab, or settings.
- **L**: several days, a private API, a separate signed target, or a security risk.

**Have:** ✅ already in Aloud · ◐ partly there · blank means new.

---

## Shared building blocks (build these once, first)

Almost every feature is one of these five plus a few lines of glue.

1. **The island queue (frontend).** Today the notch shows the agent panel, the toast and the wings. Generalise it into *live activities*: a typed `Island { kind, priority, icon, title, detail, actions, expires }` that any crate can push. Then build one renderer for the collapsed pill (several side by side, ranked) and one for the expanded panel. This is "Multiple Live Activities" below, and it's the foundation for about 30 other features. It reuses `NotchToast` and `useNotchPanel`, and the motion rules in `aloud-motion` still apply.
2. **Event sources (Rust).** Every feature takes its input one of four ways:
   - **Polling** a cheap value every 1–5 s: battery, `ioreg`, CoreAudio "is running", stocks.
   - **FSEvents** through the `notify` crate: Downloads, screenshots, `/Volumes`.
   - **CGEventTap** in `crates/hotkey`, which already swallows and reads keys: HUD keys, Caps Lock, the keystroke HUD, the cleaning lock.
   - **The bridge** (`crates/bridge`, `bridge.sock`), which already carries agent hooks into the app: shell activity, the developer API.

   Put each watcher in the crate that owns the domain, behind `#[cfg(target_os = "macos")]`, and have it emit an `Island`.
3. **Native calls.** Prefer, in this order:
   - a CLI Apple ships;
   - JXA through `script::run_jxa` (one JSON argument, never interpolated);
   - `objc2` bindings, already a dependency (the notch measure);
   - a tiny Swift helper binary bundled in `resources/`, like `TraversalTool` and `InputControllerTool`, for Vision, EventKit and other Swift-only APIs.
4. **The ⌃⌥K command palette.** One registry of actions (`id`, title, keywords, `run`), shared by the palette, the voice agent and the island buttons. Many features are just a palette action plus an island.
5. **Settings and permissions.** Every feature gets an on/off row; most default to off. Permissions already have a pane (`crates/permissions`). The new ones needed are Full Disk Access (Notification Mirror, Focus), Camera, and Input Monitoring (Keystroke HUD).

---

## Deep work

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Pomodoro Timer** | Timer state in Rust (`tokio::time`, keyed to wall clock so sleep doesn't drift). A collapsed-pill island with the countdown, and an expanded tab with session chaining. Streaks and a 7-day chart come from a small table in `crates/store` (`timings.sqlite` pattern). | M | |
| **Focus Mode** | There's no public "current Focus" API. Read `~/Library/DoNotDisturb/DB/Assertions.json`, which needs Full Disk Access, or run a one-action Shortcut, "Get Current Focus", via `shortcuts run`. Poll every few seconds and show an island on change. While Do Not Disturb is on, drop low-priority islands. | M | |
| **Keep Awake** | `caffeinate -d -t <secs>` as a child process (kill it to stop), or `IOPMAssertionCreateWithName` through `objc2`/IOKit. Durations, "until 5pm" and auto-off are just the timer. | S | |
| **Quick Notes** | `crates/notes` already stores markdown notes. Add a notch tab with a textarea that autosaves; voice can already create notes. | S | ◐ |

## Music and media

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Now Playing** | **Spotify and Music:** AppleScript (`player state`, track, artwork URL, `player position`), already used by `ALOUD_SPOTIFY`/`ALOUD_MUSIC`. Poll every 1 s while playing. **Any app:** MediaRemote is private, and from macOS 15.4 only Apple-signed processes can read it, so use the `mediaremote-adapter` approach (a helper run through `/usr/bin/perl`). **Browser video speed:** JS through AppleScript `execute javascript` in Chrome/Brave/Edge/Safari. | M (L for "any app") | ◐ |
| **Synced Lyrics** | Fetch from LRCLIB (`lrclib.net/api/get?track_name=&artist_name=&duration=`, free, no key), parse the LRC timestamps, and scroll in sync with Now Playing's position. Cache per track. | S | |
| **Audio Switcher** | CoreAudio device list plus `AudioObjectSetPropertyData(kAudioHardwarePropertyDefaultOutputDevice)`. `crates/volume` already talks to CoreAudio, and `coreaudio-sys` covers the rest. | S | ◐ |
| **AirPods Manager** | Battery: `ioreg -r -k BatteryPercent` covers Apple accessories, and `system_profiler SPBluetoothDataType -json` gives left, right and case for AirPods. Connect: `IOBluetoothDevice.openConnection` through a small Swift helper, or `blueutil --connect`. "Nearby case detected" needs BLE scanning of Apple's proximity packets in a Swift helper. | M (the nearby popup is L) | |
| **Sound Mixer** | Per-app volume and EQ need Core Audio process taps (macOS 14.4+, `AudioHardwareCreateProcessTap`) feeding an aggregate device you process. This is real audio-engine work, in a Swift helper. | L | |
| **Live Audio Spectrum** | The same process tap, or ScreenCaptureKit audio-only capture (needs the Screen Recording permission). Run an FFT with `rustfft` into about 16 bars and stream them to the pill over a channel. `crates/audio` already meters the mic. | M–L | ◐ |

## Building and shipping

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **AI Usage Tracker** | `crates/quota` already reads Claude plan usage (Keychain OAuth plus the usage API), and `QuotaDashboard.tsx` draws it. Add Codex, Cursor and Copilot from their local logs or APIs, an alert at a threshold, and provider ordering. | S–M | ✅ (Claude) |
| **GitHub PRs** | Composio GitHub is already connected, so list open PRs and checks through it, or through `gh pr list --json`. Poll every few minutes; an island when a check turns red or a review lands. | S | ◐ |
| **Notch Terminal** | `portable-pty` in Rust spawning a login shell, and `xterm.js` in a notch tab, with bytes over a Tauri channel. Keep the PTY alive across tab switches. | M | |
| **System Stats** | The `sysinfo` crate for CPU, memory and network, polled every 2 s only while the flyout is open. | S | |
| **Agent Approvals** | Already built: `crates/bridge` carries Claude Code hooks, and `PermissionCards.tsx` shows Allow/Deny that goes back through the hook. Remaining: clear the island when the user answers in the terminal instead. | S | ✅ |
| **Agent Activity** | `crates/sessions` already tracks running Claude/Codex sessions, and `NotchAgents.tsx` shows them. Add per-turn cost from the transcript's usage fields. | S | ✅ |
| **Shell Activity** | An opt-in zsh `preexec`/`precmd` hook (a line added to `~/.zshrc` with a marker, removed cleanly) writes start and end lines to `bridge.sock`. Commands over N seconds become a live island, and "finished" pops when they end. | S | |
| **Developer API** | The bridge already listens on a Unix socket. Add an opt-in `127.0.0.1` HTTP listener (`tiny_http`/`axum`) with a per-install token, so a JSON post turns into an `Island`. Off by default. | S | ◐ |

## Everyday Mac

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Clipboard History** | Poll `NSPasteboard.changeCount` every 500 ms (`crates/paste` already snapshots the clipboard). Store text, images and file URLs in SQLite with a cap, pins exempt. A notch tab lists them; clicking re-copies and auto-pastes through `aloud_paste`. Skip items flagged `org.nspasteboard.ConcealedType` (password managers). | M | ◐ |
| **Clipboard Smart Actions** | Regex the copied text: URL becomes open, a hex colour becomes a swatch plus copy, an email becomes a compose link (`mailto:`). Bursts of copies in a few seconds stack into one entry. | S | |
| **File Shelf** | Drop files on the notch (`onDragDropEvent`; browser `onDrop` never fires for OS drops), store their paths in the app data dir, and drag them out with `crates/drag`, which already exists. Persist across relaunches. | M | ◐ |
| **AirDrop from the Shelf** | `NSSharingService(named: .sendViaAirDrop).perform(withItems:)` through `objc2`, on the main thread. Success and failure come from the delegate. | S | |
| **Image Converter** | `sips -s format jpeg/png/heic/tiff in --out out` (already used for icons), with the result dropped back on the shelf. | S | |
| **Zip & Unzip** | `ditto -c -k --keepParent <files> out.zip` and `ditto -x -k in.zip <dir>`. | S | |
| **Window Snapping** | Moving windows: Accessibility `AXPosition`/`AXSize` (`ALOUD_WINDOW_ARRANGE` already does this). Drag detection: a listen-only CGEventTap for left-mouse drags that start in a title bar and reach the top edge. While dragging, the notch shows zones; on release, tile to the zone. | M | ◐ |
| **OCR & Search** | Apple Vision `VNRecognizeTextRequest` in a small Swift helper that takes an image path and prints text. Run it on each copied image and store the text beside the clip, so search matches words inside images. | M | |
| **Screen Capture** | `screencapture -i -c` (interactive, to clipboard) plus `screencapture -i <file>` for the shelf copy; OCR it if asked. | S | |
| **Screenshot Shelf** | Watch the screenshot folder (`defaults read com.apple.screencapture location`, default Desktop) with `notify`; each new `Screenshot*.png` becomes a draggable island. | S | |
| **Download Alerts** | Watch `~/Downloads` with `notify`, ignoring partials (`.crdownload`, `.download`, `.part`). An island with "Reveal in Finder" (`open -R`). Works for every browser. | S | |
| **Drive Eject** | Watch `/Volumes` with `notify`, or DiskArbitration callbacks. An island with Eject runs `diskutil eject <volume>`, then shows "Safe to disconnect". | S | |
| **App Launcher** | `aloud_selection::installed_apps` already returns apps with icons. A pinned grid in a tab launches with `open -a` (`ALOUD_OPEN_APP`). Drag to reorder; persist in settings. | S | ◐ |
| **Apple Shortcuts** | `shortcuts list` and `shortcuts run "<name>"` (Aloud has a Shortcuts tool), shown as a grid of chips with a running spinner and done/failed. | S | ◐ |
| **Apple Reminders** | `ALOUD_REMINDERS_ADD` already adds. A "due today" list is JXA against Reminders (`reminders.whose({completed:false, dueDate<…})`). | S | ◐ |
| **Menu Bar Calendar** | Upcoming events through JXA to Calendar.app, which covers iCloud, Google and Exchange accounts added in System Settings. EventKit would be faster but needs a Swift helper and the calendar permission. Composio Google Calendar is the fallback. | S–M | |
| **Text Snippets** | Snippets stored in `crates/store`; the palette lists them, and picking one pastes it through `aloud_paste` (synthetic paste with clipboard snapshot and restore, so history isn't touched). | S | |
| **Search & Translate** | A notch text field. Search opens `https://<engine>?q=` in the default browser. Translate uses the existing LLM client (Groq/Gemini). Quick sums use `meval`/`evalexpr`. "Paste a video link and save it" means bundling `yt-dlp`; skip or make it opt-in. | S (video is M) | |
| **Command Palette** | A global hotkey (`crates/hotkey`) opens the notch with an input and fuzzy-searches the shared action registry (building block 4). ⌘1–9 switch tabs. | M | |
| **Live Weather** | Open-Meteo (free, no key) for the forecast. Location from CoreLocation (Swift helper plus permission) or a coarse IP lookup. Refresh every 30 min; show on the Home tab. | S | |
| **Color Eyedropper** | `NSColorSampler().show { color in … }` through `objc2` on the main thread. Copy the hex and show a swatch island. No Screen Recording permission needed. | S | |
| **Camera Mirror** | `getUserMedia` in the overlay webview. Needs `NSCameraUsageDescription` in `Info.plist` and the camera permission. | S | |
| **Zoom Teleprompter** | A notch tab that scrolls a script at an adjustable speed (CSS transform, live spring if retargeted), sitting right under the camera. | S | |

## System HUDs and status

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **System HUDs** | The CGEventTap in `crates/hotkey` catches the media keys (`NX_SYSDEFINED` volume and brightness events) and swallows them. Volume goes through CoreAudio. Brightness uses `DisplayServicesSetBrightness`, a private API, loaded with `dlopen`. Draw the island. Swallowing the key is also what hides the stock HUD. | M | ◐ |
| **Alt HUD Styles** | CSS variants of the same HUD: edge bar, circular gauge, adjustable size. | S | |
| **Battery HUD** | `IOPSCopyPowerSourcesInfo` (the `battery`/`starship-battery` crate) or `pmset -g batt`, polled every minute; an island at low and critical. | S | |
| **Full Charge Island** | The same battery poll, firing at the chosen 80–100% with an optional sound. | S | |
| **Device Battery** | `ioreg -r -k BatteryPercent -l` lists Magic Mouse, Keyboard and Trackpad. Third-party headphones come from `system_profiler SPBluetoothDataType -json`. | S | |
| **Caps Lock HUD** | `flagsChanged` events in the existing tap (or poll `CGEventSourceFlagsState`), showing a brief island. Modifier flags need no Input Monitoring permission. | S | |
| **Mic Mute** | Set `kAudioDevicePropertyMute` on the default *input* device's input scope. That silences every meeting app at once. `crates/volume` already does the output side. | S | ◐ |
| **Privacy Indicator** | Mic: `kAudioDevicePropertyDeviceIsRunningSomewhere` on input devices. Camera: CoreMediaIO `kCMIODevicePropertyDeviceIsRunningSomewhere`. Poll every 1 s; show a green or orange dot pill. | S | |
| **Keyboard Backlight** | CoreBrightness's private `KeyboardBrightnessClient`, through `dlopen`/`objc2`. Only Macs with a backlit keyboard. | M (private API) | |
| **External Display Control** | DDC/CI over `IOAVService` on Apple Silicon (the `m1ddc` approach) for brightness. Off by default, and only for monitors that speak DDC. | L | |
| **Keystroke HUD** | A listen-only CGEventTap (Input Monitoring permission) that shows each chord as an island. Skip while `IsSecureEventInputEnabled()` is true, which covers password fields. | S | |
| **Keyboard Cleaning Lock** | The hotkey tap swallows every key until ⌘⎋, with a countdown island. | S | |

## The notch itself

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Multiple Live Activities** | Building block 1: the collapsed pill holds several islands ranked by priority (what costs most to miss) and widens to fit, keeping the never-resize-the-window canvas. | M | ◐ |
| **Peek on Hover** | Already merged (`feat/notch-hover-peek`). Add Now Playing's transport controls to the peek. | S | ✅ |
| **Gestures & Hover Open** | `wheel` events with horizontal delta on the notch swipe between tabs; hover-to-open already exists; auto-collapse on pointer leave with a delay. Each gets a settings toggle. | S | ◐ |
| **Pill on Any Mac** | The drop-down pill style already floats where the notch would be on Macs without one. Add size and offset settings and a target-display choice. | S | ✅ |
| **Liquid Glass UI** | The overlay already has a liquid-glass style. Add panel width and height settings with a live preview. | S | ✅ |
| **Hide the Notch** | Draw a black strip the height of the menu bar across the notch screen at a level above the wallpaper and below the menu bar text, or set a generated wallpaper with a black top band. | M | |
| **Hidden from Screen Capture** | Set `NSWindow.sharingType = .none` on the overlay windows through `objc2`. A few lines behind a toggle. | S | |
| **Custom Idle Animation** | `lottie-web` in the overlay plays a user-chosen JSON file as the closed-notch idle, with speed, scale and offset settings. | S | |
| **Timer Mascots** | `PetMascot.tsx` and the cats already exist; let a mascot ride the timer island. | S | ✅ |
| **Menu Bar Detach** | A Tauri tray icon opening a small popover window that hosts the same tab component (Clipboard, Timer, AI Usage), plus a Caffeine toggle item. | M | |
| **Menu Bar Icon Hiding** | The Hidden Bar technique: two `NSStatusItem`s (a chevron and a separator). Setting the separator's `length` to about 10,000 pushes everything left of it off screen; the chevron toggles it back to its normal width. | M | |
| **Desktop Widgets** | WidgetKit needs a Swift widget-extension target signed inside the app bundle, which Tauri doesn't build. Possible as a separate Xcode target sharing data through an App Group. | L | |
| **Lock Screen Widgets** | Drawing over the lock screen needs a window at the shielding level and private behaviour; the lock screen is its own session. Risky and fragile across macOS versions. | L | |
| **Face Unlock** | Apple Vision face matching plus a liveness check, then typing the stored password into loginwindow. **Skip:** storing the login password and auto-typing it is a security liability, and macOS updates break it. Touch ID already exists. | L | |
| **100% Native** | Not a feature to copy. Aloud is Rust plus a WebView by design; the equivalent goal is idle cost (no polling while nothing is visible, and `sysinfo` only while open). | n/a | |
| **Automatic Updates** | `tauri-plugin-updater` is registered but inert. It needs a signing key pair, a pubkey in `tauri.conf.json` and a release feed. | S | ◐ |

## Calls, messages and notifications

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Message Island** | An Accessibility observer (`AXObserver`) on Notification Center's banner windows (`com.apple.notificationcenterui`) reads sender and body. Reply inline through Messages AppleScript `send` (Aloud's `ALOUD_MESSAGES_SEND` path). No Full Disk Access needed. | M | ◐ |
| **Call Island** | FaceTime and phone calls: the same AX observer on the incoming-call banner, with Answer and Decline pressing its buttons. Zoom, Teams and WhatsApp: the frontmost or running app plus the mic in use (Privacy Indicator) means "on a call", shown with a running duration. | M | |
| **Notification Mirror** | Read Notification Center's database (`~/Library/Group Containers/group.com.apple.usernoted/db2/db`, SQLite; needs Full Disk Access) into a notch tab. Opt-in. | M | |

## Money and misc

| Feature | How Aloud would build it | Effort | Have |
|---|---|---|---|
| **Stocks Ticker** | A free quote API (Finnhub key or Yahoo's unofficial endpoint) polled every minute during market hours. The pill rotates pinned symbols; alerts fire on thresholds. Label pre-market, after-hours and closed so a stale price never looks live. | S–M | |
| **Knock to Control** | The double-tap-a-key mode is easy (the hotkey tap already does hold-vs-tap). Trackpad taps work as multitouch events in a tap. Knocking the chassis needs Apple Silicon's undocumented motion sensor through IOKit (`AppleSPUHIDDevice`). | S (keys) / L (knock) | ◐ |
| **Drift-free Stopwatch** | Store the start `Instant` and paused total, and compute elapsed on each frame. Nothing ticks while nothing is visible. | S | |

---

## Suggested order

1. **Foundation:** the island queue (Multiple Live Activities) and the action registry and Command Palette. Everything after this is mostly glue.
2. **Quick wins, all S, mostly a CLI or one API plus an island:**
   - files: Download Alerts, Screenshot Shelf, Drive Eject, Zip & Unzip, Image Converter;
   - system: Keep Awake, Battery HUD, Full Charge, Device Battery, Caps Lock HUD, Mic Mute, Privacy Indicator;
   - small tools: Hidden from Screen Capture, Color Eyedropper, Screen Capture, Live Weather, Synced Lyrics, Audio Switcher;
   - agents and shell: Shell Activity, Developer API.
3. **Daily drivers (M):** Clipboard History plus Smart Actions, File Shelf plus AirDrop, Now Playing (Spotify and Music first), Pomodoro, Window Snapping, System HUDs.
4. **Later or opt-in:** Notch Terminal, OCR, Message and Call islands, Notification Mirror, Menu Bar Detach and Icon Hiding, Focus detection.
5. **Skip unless asked:**
   - Face Unlock (security);
   - Lock Screen and Desktop Widgets (they need Apple targets Tauri doesn't build);
   - Sound Mixer and Live Spectrum (an audio-engine project);
   - knock detection, and External Display DDC.
