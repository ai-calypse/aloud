# Computer Use

Notes toward letting the agent drive the screen — click, type, scroll — instead
of only calling APIs.

Nothing here is built. This exists so that when it is, the questions already
answered elsewhere are not answered again from scratch.

Two sources: **`../agent-notch`** (Swift, ~18k lines, ships a working
Anthropic computer-use loop) and what this repo already has. Every constant
below was read out of agent-notch's source with its justifying comment; where
the comment reads like a guess rather than a finding, it says so.

---

## What we already have

| Piece | Where | State |
|---|---|---|
| Screenshots | [`crates/screen`](crates/screen/src/lib.rs) | captures at 1024px JPEG data URI — **no consumer**, marked `ponytail:` |
| Input synthesis | [`crates/paste`](crates/paste/src/lib.rs) | `enigo` behind `on_main_thread`, keys only, no mouse |
| Tool loop | [`crates/agent`](crates/agent/src/tool_loop.rs) | 5 rounds, fail-safe write gate, Composio-only tool source |
| Permissions | [`crates/permissions`](crates/permissions/src/lib.rs) | Screen Recording + Accessibility already checked |
| Panel | [`overlay.tsx`](apps/desktop/src/overlay.tsx) | renders planned calls, live calls, results, confirmation gates |

So the UI, the permission plumbing and the event vocabulary exist. What is
missing is a mouse, a screenshot consumer, and a provider that speaks
computer-use.

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

This one is worth taking **now**, before any computer-use exists —
[`tool_loop.rs:146`](crates/agent/src/tool_loop.rs#L146) has the same hole with
a Composio surface ("drafted the email, never sent it").

**Check the cancel flag between individual tool dispatches, not just between
turns.** A six-tool round should abort after the current tool, not after all
six. `ComputerUseHarness.swift:240-244` and `:388-407`. Also true of our loop
today at [`tool_loop.rs:179`](crates/agent/src/tool_loop.rs#L179).

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

---

## The provider question

Anthropic's `computer_20250124` tool is Anthropic-specific. Agent mode is
currently on Gemini ([`agent.rs:107-108`](apps/desktop/src-tauri/src/agent.rs#L107))
because Groq's per-minute limit throttled a multi-round loop. Computer-use
forces an Anthropic client, which drags in one more thing worth taking:

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

**0 · Take the loop lessons first.** The completion verifier, the
between-tools cancel check and the retry ladder all land in
[`crates/agent/src/tool_loop.rs`](crates/agent/src/tool_loop.rs) and improve
Composio agent mode today. No screenshots, no mouse, no new provider,
`cargo test`-able. Do this whether or not computer-use ever ships.

**1 · Give `crates/screen` a real capture.** Bind
`SCScreenshotManager.captureImage` via `objc2` behind the existing
`screenshot_data_uri` signature, add a `target_snapshot(target: (u32, u32))`
returning image + transform, exclude our own overlay windows, cache the display
list for 5s. Keep the CLI path as the fallback and for Windows.

⚠️ `NSScreen`/ScreenCaptureKit calls are main-thread only — the same trap
`crates/hotkey` and the notch measurement already hit. See the `objc2`
`relax-sign-encoding` note in [CLAUDE.md](CLAUDE.md).

**2 · New crate: `crates/computer`.** Mouse move, click, drag, scroll, key
chords, type. Pure logic + one thin backend per OS, the way `crates/hotkey`
splits. On macOS every synthetic input must go through the main thread — see
the TIS/TSM abort in [CLAUDE.md](CLAUDE.md). `on_main_thread` currently lives
private inside [`crates/paste`](crates/paste/src/lib.rs#L26); it needs to move
somewhere shared or be duplicated deliberately.

An `examples/` binary that clicks a real button and types into a real app is
mandatory here — this is exactly the category of code whose unit tests pass
while the feature does not work.

**3 · An Anthropic client in `crates/llm`.** Streaming, the retry ladder above,
sorted-key encoding, cache-breakpoint rotation. Or skip it and do the
Claude Code provider instead — cheaper for the user, more moving parts for us.

**4 · A second loop, not a bigger one.** `tool_loop.rs` is Composio-shaped:
Composio discovery, Composio execution, a `read_only` hint per tool. A
computer-use loop has a fixed tool set, a screenshot after every action, and
100 turns rather than 5. Two loops that share `LoopEvent` will stay legible;
one loop with a mode flag will not.

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

**Our confirmation gate does not survive the translation.**
[`tool_loop.rs:244`](crates/agent/src/tool_loop.rs#L244) is fail-safe: anything
not explicitly `read_only` pauses for a yes. That works because a Composio tool
is a discrete, nameable act — *send this email* — and there are two or three of
them in a run.

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

---

## Cross-platform

Every constant above is macOS. The Windows path would be `SendInput` for
input and `Windows.Graphics.Capture` (or `BitBlt`) for capture, and — like
every other Windows branch in this repo — would ship unexercised until CI is
unblocked. See [PROGRESS.md](PROGRESS.md).
