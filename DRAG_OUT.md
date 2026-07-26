# Dragging files out of the notch

## Goal

A file the agent surfaces — a Finder-search thumbnail, a generated note, a
screenshot — can be **dragged from the notch into any other app** (Finder,
Slack, Mail, a chat box) and lands there as a *real file*, exactly as if dragged
from Finder.

## Why this is the one hard part

Tauri's drag API is one-directional. `onDragDropEvent`
([`tauri-wisdom/frontend/drag-drop.mdx`]) handles files dropped **into** the
WebView; there is **no Tauri API for dragging out**. So this is native macOS
work: a system drag must originate from an `NSView` as an
`NSFilePromiseProvider` (or a file-URL pasteboard item) so the receiver treats
it as a genuine file.

**The load-bearing constraint** (verified in `objc2-app-kit-0.2.2`):
`beginDraggingSessionWithItems:event:source:` requires the **live mousedown
`NSEvent`**. A drag cannot be started from a Tauri command fired by a JS
`click`/`dragstart` — by the time the IPC round-trips, the OS mouse event is
gone and no drag attaches. This kills the "JS calls `start_drag(path)`" shape
that looks obvious.

## The design fork

### Option A — ride WebKit's own drag (try first, cheap)

WKWebView **already** starts a real `NSDraggingSession` for a `draggable` HTML
element, bridging HTML5 drag → AppKit internally. If we render each result as a
`draggable` element whose drag data is a `file://` URL (or better, a
`text/uri-list` / `NSFilesPromise`), WebKit may hand the receiver a real file
with **zero Rust and no event interception**.

- Cost: a `dragstart` handler setting `dataTransfer` — pure frontend.
- Risk: WebKit may only export image *bytes* or a URL string, which Finder
  accepts but Slack/Mail may not treat as a file. **Unknown until tested on the
  running app** against real drop targets.
- The asset-protocol scope (`$DATA/Aloud/media/*`,
  [`tauri.conf.json`](apps/desktop/src-tauri/tauri.conf.json#L47)) does not cover
  arbitrary search-result paths, so a `file://` ref to `~/Desktop/foo.png` is
  outside the WebView's allowed origins — this may block the ride-along and
  force Option B.

**Decision: spend 30 minutes proving or killing Option A before building B.**
It is the difference between a 20-line diff and a native AppKit feature.

### Option B — native NSDraggingSession (the robust path, chosen)

If A doesn't yield a real file, start the drag natively. The mousedown-event
constraint means JS cannot trigger it directly, so we **bridge**:

1. **Frontend** — on `mousedown` over a draggable result, before anything else,
   `invoke("arm_file_drag", { path })`. This stashes the path in Rust
   `AppState` (a `Mutex<Option<PathBuf>>`). It does *not* start the drag.
2. **Native interception** — a subclassed or event-monitored `NSView` over the
   WebView (or the WebView's own view) catches the following `mouseDragged`,
   reads the armed path, and calls
   `beginDraggingSessionWithItems:event:source:` with an
   `NSFilePromiseProvider` for that path and the live event.
3. **Drag source delegate** — supplies the file to the receiver (a promise that
   copies/points to the real path) and clears the armed path when the session
   ends.

**The traps, from CLAUDE.md and the notch code:**
- **Main thread only.** All AppKit here runs on the main thread —
  [`overlay.rs:83`](apps/desktop/src-tauri/src/overlay.rs#L83) already marshals
  notch work there; the drag view setup joins it. A drag call off-thread is the
  `dispatch_assert_queue` SIGTRAP class of bug.
- **`relax-sign-encoding`** is already enabled on the shared `objc2`
  ([`Cargo.toml`](apps/desktop/src-tauri/src-tauri/Cargo.toml#L74)); the drag
  classes need no new feature flag beyond adding
  `NSDragging`/`NSPasteboard`/`NSFilePromiseProvider` to `objc2-app-kit`.
- **The WebView eats mouse events.** WKWebView handles its own mouseDown, so a
  sibling overlay `NSView` must sit above it *only while a drag is armed*, or it
  steals clicks from the panel. Arm-on-mousedown/disarm-on-mouseup keeps it
  scoped. This interaction is the single biggest unknown in B and gets a
  hardware test first.

## Where it plugs in

- **The draggable surface** is the Finder grid tile
  ([`overlay.tsx` `FinderGrid`](apps/desktop/src/overlay.tsx)) first, then any
  agent-surfaced path. Generalise by carrying a `path` on the result model, not
  by special-casing the grid.
- **A `crates/drag` crate** owns the AppKit code behind `#[cfg]`, Windows a
  no-op — same shape as `crates/native`. `src-tauri` wires `arm_file_drag` and
  the view setup, since the `NSWindow`/`NSView` handle lives there
  ([`overlay.rs`](apps/desktop/src-tauri/src/overlay.rs)).

## Build order

1. **Prove/kill Option A** — a `draggable` grid tile with a `file://`
   `dataTransfer`, dragged into Finder AND Slack. If both accept a real file,
   ship that and stop. (Frontend only.)
2. If A fails: **`arm_file_drag` command + AppState stash.** No drag yet; just
   prove the path arrives on mousedown.
3. **The native drag view** — `NSFilePromiseProvider` from the armed path,
   `beginDraggingSession` on the intercepted drag event. First target: drag one
   fixed test file into Finder.
4. **Wire the grid** — mousedown arms the tile's path; verify dragging a search
   result into Finder, Slack, Mail.
5. **Generalise** — any agent-surfaced path becomes draggable via the same
   result-model `path`.

Steps 1 and 3 are the whole risk. Everything else is wiring.

## Verifying

Native drag cannot be unit-tested — it needs real drop targets. Per CLAUDE.md,
this gets hand-verification in the PR body:

- Drag a search result into **Finder** → a real file copy appears.
- Drag into **Slack** and **Mail** → attaches as a file, not a broken image.
- Drag and release **inside** the notch → no-op, panel still interactive.
- Dictate/click the panel normally → the armed-drag view never steals a click.

## Out of scope

- Windows (`crates/drag` returns/does nothing, like every other Windows path).
- Dragging *multiple* files at once — one file first; the promise API extends to
  many, but prove one.
- Dragging non-file agent output (a text answer) — different pasteboard type,
  separate build.
