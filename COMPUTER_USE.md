# Computer Use

> **Superseded (2026-10).** The Layer 2 described below — `crates/computer` over
> the vendored MacosUseSDK, and later the engine's Jev click loop — has been
> removed. Mac apps are driven by scripted intent tools in `crates/native`, and
> screen control is back as the last tier on trycua's Cua Driver (MIT,
> `libs/cua-driver`) in `crates/screen`. The rest of this document is kept as
> the record of what was learned.

Notes toward letting the agent drive the screen — click, type, scroll — instead
of only calling APIs.

The screen-driving half is not built. What *is* built, on the
`feat/native-agent-tools` branch, is **Layer 1** — the structured native-tool
path this document positions computer-use against ([`crates/native`](crates/native/src/catalogue.rs):
Music, Reminders, Notes, Messages, Finder search + reveal, volume, shortcuts).
This exists so that when Layer 2 is built, the questions already answered
elsewhere are not answered again from scratch.

Two sources: **`../agent-notch`** (Swift, ~18k lines, ships a working
Anthropic computer-use loop) and what this repo already has. Every constant
below was read out of agent-notch's source with its justifying comment; where
the comment reads like a guess rather than a finding, it says so.

A third and fourth source were evaluated 2026-07-25 (external, not vendored) —
see **[External SDKs](#external-sdks)** below. The short version: **MacosUseSDK**
(mediar-ai, MIT, Swift) does Accessibility-tree element targeting — read any
app's real buttons/fields and click *them*, not pixels — and reshapes the
primary mechanism here from vision-first to **AX-first, vision as fallback**.
The decision (from the user) is to **bundle MacosUseSDK as a signed Swift
sidecar** rather than port its AXUIElement surface to `objc2` Rust. **altic-mcp**
(Python, `osascript`) is *not* adopted — it is the same AppleScript mechanism we
already have, kept only as a catalogue of actions worth adding to
`crates/native`.

---

## Why this exists: the two-layer architecture

The trigger for this work was a concrete failure of the structured-tool
approach. `ALOUD_FINDER_SEARCH` works, but every phrasing needs the model to map
words to parameters — "screenshots" → `kind:screenshot`, "pictures" →
`kind:image`. When the mapping is wrong the search fails, and **you cannot
hand-write a tool description for every phrasing of every workflow.** There are
thousands, and hand-tuning is a treadmill.

The fix is **not a bigger tool library.** A 90-tool catalogue (altic) hits the
same wall — "pictures" vs "photos" vs "images" is a description-matching problem
in *any* fixed-tool design. The fix is a **second, general capability layer**
that needs no per-workflow code. Every serious voice-agent-for-Mac product has
converged on the same two layers:

```
Voice → LLM
         ├── Layer 1  structured tools (crates/native)      common · fast · safe · gate-able
         └── Layer 2  AX-tree computer-use (MacosUseSDK)     the long tail · zero per-workflow code
```

**Layer 1 — structured tools** (already built). Deterministic, confirmable, and
better than computer-use *where a named tool exists*: sending a message, playing
music, a Finder search. These stay. Keep adding the high-frequency, high-safety
ones (the altic catalogue is the backlog).

**Layer 2 — Accessibility-tree computer-use** (this document). The agent reads
the *actual on-screen elements* of whatever app is focused — real button titles,
roles, and rects from the AX API — and acts on *them*. **No workflow is
pre-defined.** "Rename that file", "reply to this in Slack", "toggle dark mode
in Settings" all run through one mechanism, because the agent reads the UI
instead of matching a hand-written description. This is what makes "we can't
cover thousands of workflows" go away.

### The evidence — everyone converges here

| Product | Approach | Source |
|---|---|---|
| **HeyClicky** (YC S26, ~$10M) | AX API reads real buttons + Claude vision — *"reads the actual on-screen buttons rather than pixel-coordinate guessing"* | [hokai.io](https://hokai.io/hub/tools/heyclicky), [explainx.ai](https://explainx.ai/blog/heyclicky-voice-control-mac-gpt-realtime-2-demo-2026) |
| **voice-os** (per-simmons, MIT) | `agent-desktop` *"drives Mac apps via the macOS accessibility tree (no screenshots)"* + AppleScript for scriptable apps | [github.com/per-simmons/voice-os](https://github.com/per-simmons/voice-os) |
| **Fazm** | *"Controls your entire macOS desktop using the Accessibility API"*, local-first | [fazm.ai](https://fazm.ai/macos-ai-agent) |
| **MacosUseSDK** (mediar-ai, MIT) | `traverseAccessibilityTree(pid:)` → elements with roles/rects, then `clickMouse(at:)` — the SDK we adopt as Layer 2 | [github.com/mediar-ai/MacosUseSDK](https://github.com/mediar-ai/MacosUseSDK) |

The academic framing matches: the AX tree *"converts a human-facing visual
interface plane into a machine-readable agent interface plane without requiring
the application to be redesigned"* and *"removes the [hardcoded-intent]
limitation by providing a more general interface"* — and the structured
"blueprint" of elements is cheaper to send than a screenshot and easier to
validate. See [LUMOS](https://arxiv.org/html/2606.30697v1) and
[Rethinking OS Interfaces for LLM Agents](https://os-for-agent.github.io/papers/AgenticOS_2026_paper_9.pdf).

**The catch nobody solves:** a structured tool has a clean "confirm before
send"; a computer-use run is a hundred clicks with no such boundary. That gate
is the genuinely unsolved problem — see [The open question](#the-open-question).
It is the reason Layer 2 is built carefully and Layer 1 keeps every action where
determinism and a real confirmation actually matter.

---

## What we already have

| Piece | Where | State |
|---|---|---|
| Layer-1 native tools | [`crates/native`](crates/native/src/catalogue.rs) | **built** — Music/Reminders/Notes/Messages/Finder/volume/shortcuts, dispatched by `run_native` in the loop, behind the write gate |
| Layer-2 AX control | [`crates/computer`](crates/computer/src/lib.rs) | **built** — read/click/type/key over the vendored MacosUseSDK helpers, gated once per run |
| Input synthesis | [`crates/paste`](crates/paste/src/lib.rs) | `enigo` behind `on_main_thread`, keys only, no mouse |
| Tool loop | [`crates/agent`](crates/agent/src/tool_loop.rs) | 5 rounds, fail-safe write gate, **two tool sources** (Composio + native), completion verifier and between-tools cancel **already landed** |
| Permissions | [`crates/permissions`](crates/permissions/src/lib.rs) | Accessibility + Microphone checked; **Screen Recording and Automation are NOT yet variants here** |
| Panel | [`overlay.tsx`](apps/desktop/src/overlay.tsx) | renders planned calls, live calls, results, confirmation gates, and the Finder result grid |

So the UI, the permission plumbing, the event vocabulary, the loop hardening,
**and a working structured-tool layer** exist. What is missing for computer-use
is a screenshot capture, a mouse, the AX-tree sidecar (Layer 2), and a provider
that speaks computer-use.

⚠️ **No `crates/screen` exists** — screen capture for computer-use is unbuilt,
not "captures at 1024px with no consumer" as an earlier draft claimed. Building
step 1 below starts that crate from scratch. Likewise Screen Recording is *not*
yet a `Permission` variant (only Accessibility + Microphone are), so the
capture step adds it.

---

## What to take from agent-notch

### Capture

**Pin the model's coordinate space to 1280×800.** Center-crop the display to
16:10, then scale. Their comment claims Anthropic's computer-use models *"peak
in click accuracy at exactly this resolution"*, and that most MacBook Pro
displays are already 16:10 so the crop is a no-op there.
`ComputerUseHarness.swift:47-51`, `ScreenCapture.swift:141-144`.

**Re-derive the coordinate transform on every screenshot, not once per run.**
Their reason, verbatim: *"If the user has moved the window or switched displays
the crop region shifts — without this update clicks land on stale pixels."*
`ToolDispatcher.swift:105-117`.

**Keep two coordinate spaces straight and convert only at the boundary.** Model
space (1280×800) ↔ OS logical points ↔ backing pixels. The crop rect lives in
logical points. On Retina that is a factor of 2, and on mixed-DPI multi-monitor
it differs per display. `ScreenCapture.swift:76-105`, `:214-221`.

**1568px longest edge for any capture not in the fixed space** — *"matches the
size Anthropic auto-downsamples to anyway, so we save tokens + TTFT with zero
quality loss."* Ours is currently 1024
([`DEFAULT_MAX_DIMENSION`](crates/screen/src/lib.rs#L15)). JPEG quality 0.7
generic, 0.75 in the harness, 0.8 for the first shot of a run.

**Keep the full-resolution image alongside the downscaled one** when anything
has to read small text — they hold `Snapshot.rawImage` before the downsample
step *"so small UI text stays readable"*. `ScreenCapture.swift:47-52`.

**Cache the display enumeration with a short TTL.** `SCShareableContent` is
*"the biggest single cost in the capture path (~30-80ms cold)"*; they cache it
for 5s and invalidate on `didChangeScreenParametersNotification`.
`ScreenCapture.swift:27-45`.

They use `SCScreenshotManager.captureImage` — the one-shot ScreenCaptureKit
call, not `SCStream`. We shell out to `screencapture` (~200ms, whole display
only, cannot exclude our own overlay). The `ponytail:` at
[`lib.rs:85-88`](crates/screen/src/lib.rs#L85) already names
`SCScreenshotManager.captureImage` as the upgrade path; computer-use is what
finally justifies it.

Note they pass `excludingWindows: []` — their own notch UI is in every
screenshot the model sees. We would want the exclusion, since our overlay is
larger and sits over the content being asked about.

### The loop

**A pre-stop completion verifier.** When the model stops emitting tool calls,
do not accept the stop. One cheap call grades the agent's own final claim
against the original transcript; if a sub-goal is unfulfilled, inject a nudge
and resume. Fails **open** — any parse or network failure counts as complete,
so a broken grader cannot trap the loop. `maxVerifierRetries = 2`.
`ComputerUseHarness.swift:1000-1064`.

Built to catch a named failure: *"opened Spotify, said done, never played
Taylor Swift."* Their grader prompt:

> You grade whether an on-screen computer-use agent completed every part of the
> user's spoken request, based ONLY on the agent's own final claim. Be strict.
> If the request has multiple conjoined sub-goals (e.g. "open X AND play Y" /
> "find A and reply with B"), every sub-goal must be explicitly confirmed in
> the agent's claim. Opening an app is NOT the same as performing the action
> inside the app.
>
> Reply with exactly one JSON object, no prose:
> `{"complete": true|false, "missing": "..."}`

✅ **Already landed on this branch.** The completion verifier is built —
[`tool_loop.rs`](crates/agent/src/tool_loop.rs) has `verify`, `MAX_VERIFIER_RETRIES = 2`,
a fail-open grader, and the send-back loop. Kept here as the record of where it
came from; a computer-use loop reuses the same grader, tightened for on-screen
sub-goals.

**Check the cancel flag between individual tool dispatches, not just between
turns.** A six-tool round should abort after the current tool, not after all
six. `ComputerUseHarness.swift:240-244` and `:388-407`. ✅ **Already done** — the
loop checks `cancelled()` between dispatches (not just per round), so Escape
stops a multi-tool round part-way. A computer-use loop inherits this.

**Loop caps:** `maxTurns = 100`, `maxOutputTokens = 4096` (raised to
`thinking_budget + 2048` when extended thinking is on). Thinking budgets:
low = off, medium = 2048, high = 8192. Model fallback fires on HTTP 400/404
only, once per run.

**Retry ladder**, per status, with jitter — ours only handles 429 and only
once:

```
429             → 3 attempts, [1.0, 4.0, 16.0] + jitter
                  or Retry-After, capped at min(retry_after + jitter, 30)
500/502/503/504 → 2 attempts, [0.25, 1.0] + jitter
529             → 3 attempts, [1.0, 4.0, 16.0] + jitter
default         → no retry
jitter = random(0…0.4)
```

Retries happen **before** the stream opens; once bytes flow, failures throw
into the stream.

### Prompt rules

**Suppress the turn-1 screenshot.** The first shot is already attached from
when the user started talking; letting the model take another burns a full
round trip before anything happens.

**Scroll calibration**, verbatim, because it names the failure it prevents:

> Use 8-10 (≈ half a screen) and 15-20 (≈ a full screen). NEVER scroll with
> amount=1 or 2 expecting visible motion; you will get stuck repeating tiny
> scrolls. If after one scroll the target still is not on screen, double the
> amount on the next call instead of repeating the same value.

The same hint is echoed back in every scroll tool result, not just stated once
in the system prompt.

**No clarifying questions, with the reason stated:** *"The user told you what
to do via voice; they're not at the keyboard to type a clarification."*
Applies to every mode in this app, not just computer-use.

**`open_app` by filesystem path, never `open_url` with a scheme.** Launch
Services resolves `discord://` to Discord **Canary** when both are installed.
Candidates: `/Applications/{n}.app`, `/System/Applications/{n}.app`,
`/System/Applications/Utilities/{n}.app`, `~/Applications/{n}.app`. The rule is
repeated in three places so the model cannot miss it.

### Input synthesis

**Type via the pasteboard past a threshold.** Text longer than 4 chars and
all-ASCII: snapshot the pasteboard, write, post ⌘V, restore after **200ms** —
*"so the receiving app has time to read our string before it's clobbered."*
Shorter or non-ASCII goes per-character. `ToolDispatcher.swift:374-410`.

We already have the snapshot/restore half in
[`crates/paste`](crates/paste/src/lib.rs#L100-L134); this is the same trick
pointed at a different caller.

**Chromium and Electron cannot be clicked the normal way.** `CGEvent.postToPid`
silently no-ops on Chromium's GPU process tree — *"if the click visibly does
nothing we cannot detect that synchronously"*, so they predict it from a
hardcoded bundle-ID set plus a `com.electron.` prefix and skip straight to a
private SkyLight SPI. Chrome, Arc, Slack, Discord and VS Code all land there.

Do not port the SkyLight path — it is `dlsym` into a private framework with a
yabai-derived event-record layout. Port the *knowledge* that browser and
Electron clicks are the hard case, and consider CDP against a real browser
instead of pixel-clicking one.

**Set `localEventsSuppressionInterval = 0`** — macOS WindowServer otherwise
suppresses synthetic moves while the user is also touching the trackpad. And
give synthetic events a stable `sourceStateID` so our own event tap can skip
them and not feed itself.

### The Accessibility tree is a second sense

`AXFastPath` queries the Accessibility API for element names, roles and rects
instead of looking at pixels — bounded at 1500 nodes, depth 25. Gate an
`AXPress` on `AXUIElementCopyActionNames` actually containing `AXPress`:
*"Some elements return .success on a synthesis call but do nothing visible."*

Cheaper and more precise than vision where it works, useless where it does not
(canvas, Electron, PDFs). We already read a slice of this API in
[`crates/selection`](crates/selection/). Worth having as the fast path with
screenshots as the fallback, not as an alternative to them.

→ This is exactly what **MacosUseSDK** provides pre-built (see
[External SDKs](#external-sdks)); the decision is to adopt it as the Layer-2
fast path rather than grow our own `AXFastPath` from the `crates/selection`
slice.

---

## External SDKs

Two off-the-shelf projects were evaluated as a way to not start from scratch.
They sit at opposite ends of usefulness.

### MacosUseSDK — the one worth taking

`github.com/mediar-ai/MacosUseSDK` (**MIT**, Swift, standalone Swift package;
`mcp-server-macos-use` is a thin MCP wrapper over it — we want the SDK, not the
wrapper). It is **Accessibility-tree element targeting**, which is a different
and better mechanism than everything above:

- `traverseAccessibilityTree(pid:onlyVisibleElements:)` → every UI element with
  its **role, title, and rect**, as structured data. The agent reads the actual
  *Send* button, not a screenshot of one.
- `clickMouse(at:)` / `clickMouseAndVisualize(at:duration:)`, plus keypress and
  text-writing, act on those coordinates.
- Public `AXUIElement`/`ApplicationServices` APIs — no private frameworks
  (unlike agent-notch's SkyLight click path, which this doc already says not to
  port).

This is exactly the **"Accessibility tree is a second sense"** section above,
already built and MIT-licensed. Where that section says *"worth having as the
fast path with screenshots as the fallback"*, MacosUseSDK **is** that fast path.
It flips the primary mechanism: **AX-first element targeting, vision only where
AX is blind** (canvas, Electron, PDFs — the same limits noted above).

**Decision (user, 2026-07-25): bundle it as a signed Swift sidecar**, not port
its AXUIElement surface to `objc2` Rust. Rationale: the SDK *is* AppKit, a Rust
port is a large `objc2` reimplementation, and the SDK is MIT so vendoring the
binary is clean. The cost is the one CLAUDE.md flags — a second-language signed
binary in the `.dmg` and a subprocess to manage — accepted here deliberately
because the capability (control any app by its real elements) is worth it and
there is no AppleScript equivalent.

**What the sidecar shape has to solve** (the socket/lifecycle work is already
scoped in [Or: run it on the user's Claude subscription](#or-run-it-on-the-users-claude-subscription)
below — reuse that Unix-socket + child-process machinery; it is the same
pattern):

- Compile MacosUseSDK's CLI (`swift build -c release`) into a helper binary,
  bundle under `Resources/`, sign + notarize with the app. Tauri strips the
  target triple from sidecar names in the bundle — handle both names, per
  [CLAUDE.md] and `tauri-wisdom/architecture/sidecar-pattern.mdx`.
- Rust ↔ sidecar over stdio or a Unix socket; `traverse` returns JSON the loop
  feeds the model, `click/type` take model-space coords converted at the
  boundary (the coordinate-space rules above still apply).
- The sidecar needs Accessibility permission — already checked by
  [`crates/permissions`](crates/permissions/src/lib.rs). Screen Recording only
  when we fall back to vision.
- ⚠️ AX calls are main-thread-affine in-process, but a *separate* Swift process
  sidesteps our `on_main_thread` marshalling entirely — the same reason
  `osascript` is safe off our main thread. The subprocess boundary is a feature.

`ponytail:` bundling the prebuilt SDK binary is the fast path; a Rust `objc2`
port of just `traverse` + `click` + `type` is the upgrade if the sidecar's
signing/lifecycle cost ever outweighs the port. Revisit only then.

### altic-mcp — reference only, not adopted

`github.com/altic-dev/altic-mcp` (Python 3.13, stdio MCP, 90+ tools). Its
mechanism is **`osascript`/AppleScript** — the *same thing `crates/native`
already does_. It is a bigger catalogue of the same approach, not a new
capability, and adopting it as MCP re-buys exactly the Python-subprocess
bundle+sign burden the [freeflow plan](freeflow-agent-mode-mvp-plan.md) rules
out. **Not adopted.**

Kept as a **to-do list**: its tool surface (window move/resize/tile/center,
clipboard read/write, Safari/Chrome tab control, brightness/volume, app
launch/focus/quit) is a ranked backlog for extending `crates/native` in Rust,
and its AppleScript is worth reading before we write ours. These are native
`crates/native` additions, *not* computer-use — they belong with the Music /
Reminders / Finder catalogue, behind the same write gate.

---

## The provider question

Anthropic's `computer_20250124` tool is Anthropic-specific. Agent mode is
currently on Gemini ([`agent.rs`](apps/desktop/src-tauri/src/agent.rs) —
`GEMINI_MODEL = "gemini-3.1-flash-lite"`) because Groq's per-minute limit
throttled a multi-round loop. Computer-use forces an Anthropic client, which
drags in one more thing worth taking:

**Prompt-cache breakpoint rotation.** Anthropic allows 4 `cache_control`
breakpoints per request. Walk the history each turn, stamp the last
cache-eligible block of the **3 most-recent** user messages, strip markers from
older ones; the 4th sits on the system block, which also covers the tool array
because the request prefix hashes front-to-back.
`ComputerUseHarness.swift:806-852`.

The paired constraint, and a real bug someone hit: the JSON encoder must emit
**sorted keys**. *"Anthropic hashes the raw request prefix… without this every
turn's cache key would differ and `cache_read_input_tokens` would always be
0."* `AnthropicClient.swift:42-50`.

### Or: run it on the user's Claude subscription

Their second provider spawns the user's own CLI —

```
claude -p --output-format stream-json --mcp-config <inline JSON> --allowedTools "mcp__x__*"
```

— and routes the tool calls back into the app through a bundled MCP stdio
sidecar over a Unix domain socket. The run bills against the user's Claude Code
auth; no API key needed.

~1,530 lines in Swift, most of it raw `Darwin.socket` that
`tokio::net::UnixListener` collapses to a fraction. **Take the architecture,
not the bytes.** What is worth carrying over:

- Socket at `/tmp/<app>-mcp-<pid>.sock`, `unlink()` before `bind()` to clear
  crashed-instance leftovers, `chmod 0600` after.
- One client at a time; a second connection is accepted and immediately closed.
  The harness needs a re-entry guard or a second `claude` gets spawned and
  rejected.
- **`stdin = /dev/null`** — *"leaving it inherited can cause CC to hang waiting
  on a TTY in some shells even when `-p` is used."*
- Sanitize the child env: allowlist `HOME USER LOGNAME SHELL LANG LC_ALL TERM
  TMPDIR`, hardcode `PATH=/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin`, pass
  `CLAUDE_*` and `NPM_*` through, and **strip `ANTHROPIC_API_KEY`** so it uses
  the user's auth rather than ours.
- Watchdog at **90s** of silence, polled every 5s — *"CC's SessionStart hooks +
  cold cache can take 30s easily; pick 90 to stay well clear of normal startup
  but still surface a real hang."*
- Rotate the session at **150,000 tokens** — 75% of the 200k window, *"a clean
  session boundary well before CC's own auto-compact would kick in."*
- Treat exit code **15** (SIGTERM) as success alongside 0, so the kill switch
  does not produce a spurious error.
- Parse both CC's envelope format and raw Anthropic streaming events — the
  format drifts between CC versions.

---

## How to build it here

In dependency order. Each step is useful on its own; nothing below needs the
step after it.

**0 · Loop lessons — mostly done.** The completion verifier and the
between-tools cancel check **already landed** on this branch (see the ✅ marks
above). What remains is the **retry ladder** (per-status backoff with jitter) in
[`crates/agent/src/tool_loop.rs`](crates/agent/src/tool_loop.rs) / the LLM
client — still worth doing, improves Composio + native agent mode today, no
screenshots or mouse needed.

**1 · Give `crates/screen` a real capture.** Bind
`SCScreenshotManager.captureImage` via `objc2` behind the existing
`screenshot_data_uri` signature, add a `target_snapshot(target: (u32, u32))`
returning image + transform, exclude our own overlay windows, cache the display
list for 5s. Keep the CLI path as the fallback and for Windows.

⚠️ `NSScreen`/ScreenCaptureKit calls are main-thread only — the same trap
`crates/hotkey` and the notch measurement already hit. See the `objc2`
`relax-sign-encoding` note in [CLAUDE.md](CLAUDE.md).

**2 · Bundle the MacosUseSDK sidecar (AX-first path).** ✅ **Built** —
`crates/computer`, with the SDK vendored at `vendor/MacosUseSDK` (MIT) and
compiled by `build.rs`. Four tools reach the model: `ALOUD_SCREEN_READ`,
`_CLICK`, `_TYPE`, `_KEY`.

Three decisions differ from what this section originally proposed, each for a
measured reason:

- **One-shot subprocess, no socket.** The SDK's CLIs (`TraversalTool`,
  `InputControllerTool`) exit after each command, so the Unix-socket lifecycle
  scoped below buys nothing: there is no persistent state to hold. Spawn cost is
  ~30ms against a traversal of ~450-800ms and a model round-trip far longer than
  either. `helper.rs` is ~150 lines where the socket version was several hundred.
- **The traversal must be summarised, not forwarded.** Measured on a real Finder
  window: the full tree is 269 elements / ~26KB / **~8k tokens per turn**, and
  **235 of those 269 are `AXMenuItem`s inside closed menus** — real elements that
  do nothing when clicked, because the menu is not open. Filtering to labelled,
  on-screen, actually-clickable elements gives 27 elements / 2.3KB, an **11.5x
  reduction**. That filter is `tree.rs`, and it is what makes a traversal per
  turn affordable. Coordinates are emitted as the element's *centre*; the origin
  of a wide control is its left edge.
- **`--visible-only` is not optional**, for the same reason.

⚠️ **A pre-existing bug surfaced here.** `aloud_selection::debug::frontmost_pid()`
reads `AXFocusedApplication` off the system-wide element, which requires the
*calling process* to be AX-trusted and returns -25204 (`kAXErrorCannotComplete`)
when it is not — which is every `cargo run` binary, since the grant belongs to
the terminal and does not inherit. `crates/computer` falls back to System Events
(`unix id of first application process whose frontmost is true`), which needs no
such trust and was verified correct on this machine while the AX path failed.
The AX path is kept first because it is right inside the signed app and costs no
subprocess. **The `frontmost` example is still broken for the same reason** and
was left alone; fixing it is not this change's job.

**Verified on real hardware** via `cargo run -p aloud-computer --example screen`:
traversed Finder, Safari and TextEdit; the read follows the focused app; typing
landed in a real TextEdit document; clicking the Bold checkbox at its reported
coordinates changed the document's font run to `Helvetica-Bold`. That last one
is the load-bearing check — an earlier "success" was the helper exiting 0 with
the text going nowhere, because TextEdit had no open document.

**2b · Rust mouse crate as the vision fallback (later).** Only where AX is blind
(canvas, Electron, PDFs): mouse move/click/drag/scroll via `objc2`/`CGEvent`,
pure logic + thin OS backend like `crates/hotkey`. On macOS every synthetic
input goes through the main thread — the TIS/TSM abort in [CLAUDE.md](CLAUDE.md);
`on_main_thread` currently lives private inside
[`crates/paste`](crates/paste/src/lib.rs#L26) and would need to move somewhere
shared. Deferred behind step 2 — the sidecar covers the common case, and this is
only the escape hatch.

**3 · An Anthropic client in `crates/llm`.** Streaming, the retry ladder above,
sorted-key encoding, cache-breakpoint rotation. Or skip it and do the
Claude Code provider instead — cheaper for the user, more moving parts for us.

**4 · A second loop, not a bigger one.** `tool_loop.rs` is structured-tool
shaped: tool discovery, per-tool `read_only` gate, native + Composio dispatch,
5 rounds. A computer-use loop has a fixed tool set, a screenshot (or AX
traversal) after every action, and 100 turns rather than 5. Two loops that share
`LoopEvent` will stay legible; bolting a mode flag onto the structured loop will
not.

**5 · Panel rendering.** [`overlay.tsx`](apps/desktop/src/overlay.tsx) already
renders planned calls, live calls and results. What it lacks is a *screen*:
computer-use wants the current screenshot with the click point marked. The
`(tool, action)` → label mapping is worth lifting —
`left_click (420, 300)` → `click`, `left_mouse_down`/`up`/`left_click` all
collapse to `click`, `mouse_move`/`cursor_position` → `move`. ~40 lines of
hand-curated vocabulary, `ToolCallStrip.swift:62-104`.

Note their strip has **no long-run handling at all** — a 50-turn run renders
identically to a 2-turn one. Ours already does more than that; do not copy
downward.

---

## The open question

**Our confirmation gate does not survive the translation.** The gate
([`is_write`](crates/agent/src/tool_loop.rs) + `confirm` in the dispatch loop)
is fail-safe: anything not explicitly `read_only` pauses for a yes. That works —
and now covers native tools too, since they carry the same `read_only` flag —
because a structured tool is a discrete, nameable act (*send this email*, *play
music*) and there are two or three of them in a run.

A computer-use run is a hundred clicks. Confirming each is unusable;
confirming none throws away the one safety property this app currently has and
puts us where agent-notch is — which is nowhere. Their entire story is two
sentences of system prompt, a 16-app AppleScript allowlist whose parser they
admit is bypassable, and a pre-model blocklist that only stops the *fast path*:

```
delete, erase, format, wipe, remove, uninstall, shut down, shutdown,
restart, log out, purchase, buy, pay, send money
```

Substring match on the lowercased transcript. A blocked phrase still reaches
the model, which still has full tool access. And `run_shortcut` executes any
Shortcut by name with no allowlist at all.

So the gate has to be re-derived at a different granularity — per *intent*
before the run, or on recognised-dangerous *targets* mid-run (a Send button, a
Delete confirmation, a payment field), not per action. **That design is not
solved here and should not be hand-waved at implementation time.** It is the
reason to build steps 0-2 first and think while they land.

**What step 2 actually shipped, and what it does not claim.** Screen control is
gated **once per run**: the first click/type/key asks, approval covers the rest
of that run, and a refusal re-arms the gate rather than leaving the run
approved (`is_screen_write` + `screen_approved` in `tool_loop.rs`, with tests).
The read is `read_only` and never gates, since it changes nothing.

This is a deliberate interim, not an answer to the question above. It is
strictly better than agent-notch's prompt-and-blocklist story, and strictly
weaker than the per-action gate the structured tools get — one "yes" authorises
every subsequent click in the run, including one the user did not picture when
they approved. What makes that tolerable for now is that the click label shows
the element's own text (`Clicking "Send"`, not `Clicking (420, 300)`), so the
first confirmation is at least readable. **The target-aware gate is still the
open problem** — a Send button, a Delete confirmation and a payment field should
re-ask mid-run regardless of the run-level approval, and nothing here does that
yet.

---

## Cross-platform

Every constant above is macOS. The Windows path would be `SendInput` for
input and `Windows.Graphics.Capture` (or `BitBlt`) for capture, and — like
every other Windows branch in this repo — would ship unexercised until CI is
unblocked. See [PROGRESS.md](PROGRESS.md).

---

## Sources

Evaluated 2026-07-25 while deciding the two-layer direction.

**SDKs we can vendor / mine**
- MacosUseSDK (Layer 2, MIT, Swift) — <https://github.com/mediar-ai/MacosUseSDK>
- mcp-server-macos-use (MCP wrapper over the SDK) — <https://github.com/mediar-ai/mcp-server-macos-use>
- altic-mcp (reference catalogue only, Python/osascript) — <https://github.com/altic-dev/altic-mcp>

**Products that converged on AX-tree computer-use**
- HeyClicky — <https://hokai.io/hub/tools/heyclicky>, <https://explainx.ai/blog/heyclicky-voice-control-mac-gpt-realtime-2-demo-2026>
- voice-os (per-simmons, MIT) — <https://github.com/per-simmons/voice-os>
- Fazm — <https://fazm.ai/macos-ai-agent>

**Research: accessibility tree as a general agent interface (no hardcoded intents)**
- LUMOS: A Semantic OS Layer for Accessibility-Grounded AI Agents — <https://arxiv.org/html/2606.30697v1>
- Rethinking OS Interfaces for LLM Agents — <https://os-for-agent.github.io/papers/AgenticOS_2026_paper_9.pdf>
- The voice AI stack for building agents (AssemblyAI) — <https://www.assemblyai.com/blog/the-voice-ai-stack-for-building-agents>
