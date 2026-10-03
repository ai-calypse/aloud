# Agent mode — the Mac and the web

The goal is one sentence: **say a task, and it happens** — whether the thing
that has to happen lives in a Mac app or on the internet.

That splits into exactly two execution paths, and deliberately not three:

- **Composio** for anything with a cloud API. Already built and working.
- **Native macOS** for anything without one. Not built. This document is mostly
  about that.

**MCP is out**, for the foreseeable future. Composio is free, managed, and
already covers the web apps we want; running our own MCP client would mean
duplicating connector management we get for nothing, and most MCP servers are
Node stdio subprocesses — a bundled runtime to sign and notarize for coverage we
already have. Revisit only if Composio has a gap that matters *and* the server
that fills it is remote/HTTP, so there is no subprocess at all.

Nothing in the native half is built. This exists so that when it is, the
questions already answered elsewhere are not answered again from scratch.

---

## The seam already exists

Adding a second execution path needs no new architecture. `ALOUD_ASK_USER` in
[`tool_loop.rs`](crates/agent/src/tool_loop.rs) is already a tool the loop owns
rather than Composio: its schema is appended to Composio's, the model calls it
like any other tool, and the loop intercepts it **by name** before
`composio.execute()` ever sees it.

Native tools are the same shape. One `ALOUD_`-prefixed namespace, one branch in
the dispatch, and everything else works unchanged:

- **The write gate.** [`is_write`](crates/agent/src/tool_loop.rs#L638) looks a
  tool up in the same `&[ToolDef]` slice and treats anything not explicitly
  `read_only` as a write. Put native tools in that slice and the confirmation
  gate applies with no new code.
- **Dedup, per-tool budget, question budget** — all keyed on tool name, all
  already there.
- **The panel.** Labels and bodies come from the registry in
  [`tool-views.tsx`](apps/desktop/src/lib/tool-views.tsx); a native tool is one
  more entry.

So there is **no Connector Registry to build**. The earlier version of this plan
proposed a `(app, action) → backend` map; that is a catalogue, and
`PINNED_TOOLS` says in its own comment that it is *"a floor, not a catalogue"*.
An `ALOUD_` prefix and a `match` do the same job for a fraction of the code.

---

## What already exists

| Piece | Where | State |
|---|---|---|
| Composio tools, ranked + pinned | [`composio.rs`](crates/agent/src/composio.rs) | working — Gmail, Calendar, GitHub, Contacts |
| The loop, gate, dedup, budgets | [`tool_loop.rs`](crates/agent/src/tool_loop.rs) | working |
| Ask-the-user mid-run | `ALOUD_ASK_USER` | working; **the template for a second backend** |
| Panel: steps, gates, questions | [`overlay.tsx`](apps/desktop/src/overlay.tsx) | working |
| Frontmost app + installed apps | [`selection`](crates/selection/src/lib.rs#L145) | working, cached |
| Accessibility / Screen Recording checks | [`permissions`](crates/permissions/src/lib.rs) | working |
| Key synthesis behind `on_main_thread` | [`paste`](crates/paste/src/lib.rs) | working, keys only |

Missing: a native tool source, and the TCC permission handling that comes with
it.

---

## Findings that drive the design

**`shortcuts list` returns nothing on a fresh machine.** Verified here: the
binary exists at `/usr/bin/shortcuts`, and the user's library is empty. So
"expose every Shortcut as a tool" cannot be the primary mechanism — it is an
escape hatch that starts empty for most people. Keep it as *one* tool
(`ALOUD_RUN_SHORTCUT`, enumerated at startup) for power users, and build the
real coverage as curated AppleScript actions.

**Curated actions, not a generic AppleScript runner.** A
`ALOUD_RUN_APPLESCRIPT(source)` tool would be enormously capable and wrong: the
model writes the code, and the confirmation gate then shows the user a script
they cannot judge. A gate you cannot read is not a gate — the same argument that
put the full recipient address on the email card. Curated per-action tools get
real labels, real bodies, and a `read_only` flag that means something.
`ponytail:` a generic runner is the upgrade path if the catalogue ever gets long
enough to be the bottleneck.

**`osascript` is a subprocess, so it is safe off the main thread.** This is not
true of everything in this space — CLAUDE.md records that TIS/TSM keyboard-layout
APIs *abort the process* off the main thread, which is why `aloud_paste` has
`on_main_thread`. Shelling out sidesteps that entirely. Do not reach for
`objc2` bindings to save the fork; the subprocess is the feature.

**Automation permission is per target app, and the first call prompts.**
`osascript` tell-application to a new app raises a TCC prompt, and a refusal
returns error **-1743** (`errAEEventNotPermitted`). That has to be detected and
turned into "Aloud needs permission to control Music — open System Settings →
Privacy & Security → Automation", not surfaced as a failed tool call. The
existing [`permissions`](crates/permissions/src/lib.rs) crate is where that
belongs.

**Only offer tools for apps that are installed.** `installed_apps()` is already
there and cached. It is the native mirror of Composio's connected-toolkits idea:
no Music on this Mac, no Music tools in the list. This also keeps the tool count
down, which matters — `TOOLS_PER_TOOLKIT` exists because three toolkits
uncapped measured ~77k tokens of schema per round.

---

## 1 · `crates/native` — the tool source

No Tauri dependency, macOS body behind `#[cfg]`, Windows returns an empty list
the way `notch.rs` does.

```
crates/native/
  src/lib.rs         tools() -> Vec<ToolDef>, execute(name, args) -> Result<Value>
  src/script.rs      osascript runner: spawn, timeout, -1743 detection
  src/catalogue.rs   the curated actions, one module-level table
  src/shortcuts.rs   `shortcuts list` / `shortcuts run`
  examples/native.rs list the tools, run one, print the result
```

`tools()` returns the same `ToolDef` the loop already consumes, so the caller
concatenates two lists and passes one slice. `execute` returns the same
JSON-shaped result Composio does, so `tool_succeeded` and the panel need no
special case.

**Tier 1 catalogue** — chosen because each is something people actually say out
loud, and none has a cloud API:

| Tool | Read-only | Notes |
|---|---|---|
| `ALOUD_MUSIC_PLAY` / `PAUSE` / `NEXT` | no | play by name needs a search first |
| `ALOUD_MUSIC_NOW_PLAYING` | yes | |
| `ALOUD_REMINDERS_ADD` | no | list name optional, defaults to the default list |
| `ALOUD_NOTES_APPEND` | no | append to a named note, create if missing |
| `ALOUD_MESSAGES_SEND` | no | the one with real blast radius — see the gate below |
| `ALOUD_FINDER_REVEAL` | no | opens a path in Finder |
| `ALOUD_SYSTEM_VOLUME` | no | |
| `ALOUD_RUN_SHORTCUT` | no | enumerated from `shortcuts list`; empty for most |

`ALOUD_MESSAGES_SEND` gets the same treatment as `GMAIL_SEND_EMAIL` did after
the wrong-recipient bug: the gate must show the **full** recipient handle, never
abbreviated, and a name that resolved to nothing must ask rather than guess.
That path is already built — reuse it.

---

## 2 · Dispatch

One branch, in the loop's tool arm, beside the existing `ASK_USER` interception:

```rust
let result = if let Some(native) = aloud_native::execute(&name, &args) { … }
             else { composio.execute(&name, &args).await };
```

Everything before it — the plan event, the dedup, the budget, the confirmation
gate — is untouched, because they all work off the tool name and the `ToolDef`
slice.

`ponytail:` a `match` on a name prefix rather than a registered handler map. The
map is the upgrade path if a third source ever appears; with two, it is
ceremony.

---

## 3 · Permissions

A native tool that fails on TCC is not a failed action, it is a missing grant,
and the difference is the whole user experience.

- Detect -1743 in `script.rs` and return a typed `NotPermitted { app }` rather
  than a generic error.
- Surface it in the panel as its own state with a button that opens
  `x-apple.systempreferences:com.apple.preference.security?Privacy_Automation` —
  the existing `open_permission_settings` command already does this shape for
  Accessibility.
- **Prompt at the moment of use, not at onboarding.** macOS only raises the
  Automation prompt when the app is first controlled, so a settings pane cannot
  pre-grant them. Trying to front-load this produces a screen full of toggles
  that do nothing.

---

## Build order

Each step is useful alone; nothing below needs the step after it.

1. **`crates/native` + `examples/native.rs`**, with two tools only —
   `MUSIC_NOW_PLAYING` (read) and `MUSIC_PLAY` (write). No app changes. Ends
   when the example lists both and runs them against real Music.
2. **Dispatch in the loop**, so a spoken "what's playing" works end to end
   through the existing panel. This is the step that proves the seam.
3. **The -1743 path**, verified by revoking Automation for Music in System
   Settings and running the same command.
4. **The rest of the tier 1 catalogue**, plus `tool-views` entries so each has a
   real label and body.
5. **`ALOUD_RUN_SHORTCUT`**, last, because most libraries are empty.

Steps 1–3 are the whole risk. After that it is filling in a table.

---

## Verifying

Unit tests do not prove OS integration, and this is nearly all OS integration.
Per CLAUDE.md, anything touching the OS gets an `examples/` binary and the
result goes in the PR body.

```sh
cargo run -p aloud-native --example native            # list, then run one
cargo test -p aloud-native                            # catalogue + parsing
```

Worth asserting in tests rather than watching:

- A tool's `read_only` flag matches what it actually does — a write mislabelled
  read-only silently skips the confirmation gate, which is the one
  non-negotiable property in this codebase.
- -1743 maps to `NotPermitted`, not to a generic failure.
- `shortcuts list` returning zero lines yields zero tools, not one broken one.

By hand, because nothing else proves it:

- Revoke Automation for Music, run the command, get the permission state and not
  an error.
- A `MESSAGES_SEND` gate shows the full handle before sending.
- Dictate while a native tool is mid-run — the overlay and the panel must not
  fight.

**Workspace gate.** CI is unbilled and red regardless, so the local run is the
gate:

```sh
cargo clippy --workspace --all-targets   # must be warning-free
cargo test --workspace
pnpm --filter @aloud/desktop typecheck
pnpm --filter @aloud/desktop test
```

---

## What this does not cover

Named so each is a decision rather than a surprise:

- **MCP, in any form.** See the top. Revisit only for a remote server filling a
  real Composio gap.
- **Driving the screen** — clicking and typing into apps with no scripting
  interface. That is [COMPUTER_USE.md](COMPUTER_USE.md), and it is a different
  problem with a different failure mode.
- **Showing coding agents in the notch** — [AGENT_NOTCH.md](AGENT_NOTCH.md).
- **A generic AppleScript runner.** Deliberate; see the findings above.
- **Local contacts via `CNContactStore`.** The `contacts` crate (0.3.9) wraps
  it, but has 191 all-time downloads — adopting it means owning it. It would fix
  name resolution at the source rather than by prompt, which is tempting; budget
  it as "adopt and probably fork".
- **Windows.** `crates/native` returns an empty list, like every other Windows
  path in this repo.
- **Chained actions across both paths** (e.g. read a calendar event, then set a
  reminder). The loop already handles multi-tool rounds, so this should fall out
  for free — but it is untested across the boundary and worth one deliberate
  test case once step 2 lands.
