# Agents in the notch

Coding agents in the notch — status while they work, a session list, and
approving what they ask to run without going back to the terminal.

**Steps 1–4 of the original plan are built** on `feat/agent-notch`:
`crates/sessions` (discovery, transcript scan, installer), `crates/bridge` +
`crates/hook-cli` (the socket and the `aloud-hook` sidecar), the wiring in
`src-tauri/src/sessions.rs`, and the session list + permission cards as tabs in
the overlay window. **The approval path does not work**, and the diagnosis
below says why. The model to converge on is open-vibe-island's — traced end to
end on 2026-07-22, and treated as the source of truth wherever this branch
diverged from it, because theirs ships and works.

Two sources informed the design, neither one copied:

- **`../../agent-notch`** (MIT, ours) — read-only inference from `ps`/`lsof` and
  transcript tails. Zero config, and structurally incapable of showing a button.
- **`../open-vibe-island`** (**GPL v3 — do not copy code**) — a hook bridge.
  Approvals, ten agents, terminal jump-back, at the cost of an installer.

The hook contracts are each vendor's public protocol, not open-vibe-island's
invention. Building against those contracts is fine. Porting GPL Swift into
this MIT repo is not: write it fresh in Rust from the protocol, never from
their files.

---

## Why the branch does not work — the diagnosis

The chain, reconstructed from the code and the live `~/.claude/settings.json`:

1. **`PermissionRequest` fires.** Open-vibe-island's whole approval system
   rides on it (24 h timeout, installed matcher `*`), and it works, shipped.
   The branch's comments claiming "`PermissionRequest` never fires on current
   Claude builds" are a misdiagnosis of bug 2.
2. **`await_decision` drops every `PermissionRequest`.**
   [sessions.rs](apps/desktop/src-tauri/src/sessions.rs) begins with
   `let Some(id) = event.tool_use_id else { return Reply::default() }` — but
   the `PermissionRequest` payload **does not carry `tool_use_id`**. Every
   real request was silently discarded: no card, no directive, and from the
   outside it looked exactly like the event never firing. (Open-vibe-island
   knows this payload gap: it recovers the id from the `PreToolUse` event it
   saw a moment earlier, via a `session_id|tool_name|tool_input-json`
   correlation cache — and it never *routes* on the tool-use id at all; its
   pending map is keyed by **session id**.)
3. **The pivot made it worse.** Believing the event dead, the branch moved the
   gate onto `PreToolUse` (`wants_gate` + `GATED_TOOLS`). That hijacks calls
   Claude would have auto-approved (`acceptEdits`, `bypassPermissions`,
   allowlists — `permission_mode` is never consulted), freezes the terminal
   with no visible prompt, and — because the installed `PreToolUse` entry
   carries no `timeout` — Claude kills the hook at its ~60 s default and
   fail-open lets the call through. Net effect: cards that die after a
   minute, a terminal that stalls, and approvals that "work" only if answered
   fast.

**The fix is to be exactly open-vibe-island:** approvals ride on
`PermissionRequest` with the 24 h timeout (already in the live settings file —
`aloud-hook` owns that event today; Open Island's own hooks are currently
uninstalled here). `PreToolUse` goes back to being a non-blocking status
event. `wants_gate` reduces to `is_permission_request`. The
`permission_mode` problem evaporates — `PermissionRequest` only fires when
Claude would have asked the user, so modes and allowlists are respected for
free, and Claude's own terminal prompt stays alive in parallel: whichever
surface answers first wins, and a stale card is cleared when a later hook
event shows the session moved on.

---

## Which agents

Four payload dialects, not ten agents. Build per **format**.

| Format | Config it installs into | Approvals? | Agents covered |
|---|---|---|---|
| **Claude** | `~/.claude/settings.json` | ✅ `PermissionRequest`, 24 h | Claude Code, Qoder (`~/.qoder/`), Qwen (`~/.qwen/`), Factory (`~/.factory/`), CodeBuddy (`~/.codebuddy/`), Kimi (`~/.kimi/config.toml`, TOML `[[hooks]]`, same payload) |
| **Codex** | `~/.codex/hooks.json` + `[features] hooks = true` | ✅ `PermissionRequest`, 1 h | Codex CLI, Codex desktop |
| **Cursor** | `~/.cursor/hooks.json` | ✅ flat `{"permission":"allow"\|"deny"\|"ask"}` | Cursor's own composer agent |
| **Gemini** | `~/.gemini/settings.json` | ❌ fire-and-forget only | Gemini CLI |

**Built: the Claude dialect end to end** (minus the working approval path
above). The Codex parser and directive encoder exist in `crates/bridge` but no
Codex installer is wired and the path is unexercised against a real Codex.
Cursor/Gemini remain out of scope until one is actually installed here.

⚠️ Codex additionally requires a manual trust review — after installing, open
`/hooks` inside Codex CLI and approve the entries. That gate is Codex's and
cannot be bypassed. Say so in the UI, or the install silently does nothing.

---

## Architecture — the target (open-vibe-island's shape, in Rust)

```
agent process (any of the six)
  │  stdin: JSON payload
  ▼
aloud-hook            crates/hook-cli — spawned per event, fails open, ~ms
  │  Unix socket: <data_dir>/bridge.sock   (crates/bridge, both ends)
  ▼
src-tauri/src/sessions.rs   socket server + 3 s discovery poll
  │  ONE store: every event, hook or discovery, mutates one session table
  │  tauri-specta events: agent-sessions / agent-permission(-resolved)
  ▼
overlay window  (tabs in overlay.tsx)
  │  resolve_permission(session_id, allow)
  ▼
aloud-hook  →  stdout directive  →  agent obeys
```

Discovery (`ps` + batched `lsof` + transcript tails) fills the gaps for agents
with no hooks installed. Hooks are the truth where present.

**The structural rule, non-negotiable in the reference:** one reducer. A hook
session is keyed by the agent's own `session_id`; discovery *enriches* it
(transcript, cwd, subagents, title) and never competes with it, matched in
priority order **session id → transcript path → tty+cwd → cwd**. The branch's
second store — a `hooked: HashMap` in `src-tauri` merged at emit time by cwd —
is where the flicker, the immortal rows and the lost row detail all come from
(§5). Move hook ingestion into `SessionStore` and all three collapse into one
fix, which also restores the repo rule that logic lives in a crate.

---

## 1 · `crates/sessions` — the model and the fallback

Built. No Tauri dependency; macOS body behind `#[cfg]`; Windows returns empty.

```
crates/sessions/
  src/lib.rs        Session, SessionStore (pids, grace polls, transcript fold)
  src/discovery.rs  ps + batched lsof → processes, cwds, transcripts, subagents
  src/scan.rs       transcript tail reading (title, prompt, running tool, busy)
  src/install.rs    pure-function settings.json editor + conflict detection
  examples/         scan.rs, install_dryrun.rs, install_apply.rs
```

What it does right (keep; each carries its test): binary-path classification
(IDE-hosted sessions appear), **one batched `lsof` per poll** (the reference
spawns one per pid with a 0.2 s timeout and a swarm under load reads as dead
there — ours is the better design, don't "fix" it), transcript by three routes
(open fd → last poll's path → cwd-derived project slug), 2-poll grace, busy
from conversational records only, orphan `tool_use` = current tool, subagents
from `subagents/agent-*.jsonl` + `.meta.json`.

**Changes this crate needs:**

- **Ingest hook events** — `SessionStore` grows `apply(event)` beside
  `poll()`, keyed and matched as in the architecture note. The 2-poll grace
  then reaps hook sessions too; today a `kill -9` (no `SessionEnd`) leaves a
  row that never clears.
- **Incremental scan.** `scan()` is `read_to_string` of the whole transcript
  every 3 s per session; transcripts reach hundreds of MB. Keep a byte offset
  per path, `seek`, parse only appended lines; a shrunk file resets to zero.
- **Retire finished subagents.** Filter to busy-or-recent (reuse
  `BUSY_WINDOW`); today every subagent ever spawned is listed forever.

`ponytail:` (standing) two subprocesses per poll — `libproc` is the upgrade
path; one row per transcript when two hookless sessions share a project —
hooks carry the identity discovery cannot observe, and now get merged in.

---

## 2 · `crates/bridge` — the socket, both ends

Built and mostly right: stable path `<data_dir>/bridge.sock`, length-checked,
line-delimited JSON, one exchange per connection, round-trip tests on the
encoders.

**Change — the gate is `PermissionRequest`, full stop.**

- `wants_gate()` becomes `is_permission_request()`. Delete `GATED_TOOLS`.
  `PreToolUse` is answered immediately with no directive (status only), like
  every other lifecycle event — which is precisely the reference's managed
  configuration.
- `timeout_for` follows: 24 h for `PermissionRequest`, 45 s for everything
  else. (Matches the sidecar's read timeout to the installed entry's
  `timeout`, as today.)
- Keep the `PreToolUse` directive encoder (`permissionDecision` vocabulary)
  compiled but unused — it is the documented fallback if a future Claude
  build changes the story, and the encoder tests are already written.
- The Claude allow/deny shapes are already correct
  (`hookSpecificOutput.decision.behavior`, `interrupt: false` on deny so one
  denial doesn't kill the turn). Add `updatedPermissions` passthrough on
  allow when "always allow" lands (§6).

**The interactive-prompt wire shapes** (traced from the reference, verified
against the shipped `claude` binary's behavior):

- **A question is not its own event.** `AskUserQuestion` arrives as a
  `PermissionRequest` with `tool_name == "AskUserQuestion"` and the questions
  in `tool_input.questions[]` — each `{question, header, options: [{label,
  description?}], multiSelect?}`; drop a question missing text, header, or
  every option. The freeform "Other" the CLI shows is injected client-side,
  not in the payload.
- **An answer is an allow with `updatedInput`**: the original input plus an
  `answers` object mapping question text → answer string, multi-select joined
  `", "`. There is no separate answer message in the protocol.
- **"Don't ask again" is an allow with `updatedPermissions`**:
  `[{type: "addRules", destination: "session", rules: [{toolName}],
  behavior: "allow"}]` — synthesized from the tool name; the payload's
  `permission_suggestions` is not needed for it.
- **Deny carries the user's words**: `{behavior: "deny", message, interrupt:
  false}` — `interrupt: true` would kill the whole turn, so it stays false.

Also carried in the payload but **never populated by the sidecar**:
`terminal_app` / `terminal_tty` / `terminal_session_id` / `terminal_title`.
Filling them is §3; they are the whole basis of jump-back (§8).

---

## 3 · `crates/hook-cli` → the `aloud-hook` binary

Built: `ALOUD_SKIP_HOOKS` short-circuit, `--source` dialect flag (unknown
refused), 200 ms connect timeout, long read timeout on gating events / 45 s
otherwise, panic hook so a backtrace can never reach the agent's stdout, fail
open on every path. Installed to `<data_dir>/bin/aloud-hook`, outside the
bundle.

**To add — terminal enrichment, env-var tier only** (the reference does this
in its hook CLI; the sidecar inherits the agent's environment):

| Check, in order | Terminal |
|---|---|
| `TERM_PROGRAM` = `Apple_Terminal` / `iTerm.app` / `WarpTerminal` / `ghostty` / `WezTerm` / `vscode` | authoritative |
| …`vscode` + `CURSOR_TRACE_ID` present | Cursor, not VS Code |
| `__CFBundleIdentifier` = `com.anthropic.claudefordesktop` | Claude desktop |
| fallbacks: `ITERM_SESSION_ID`, `WARP_IS_LOCAL_SHELL_SESSION`, `GHOSTTY_RESOURCES_DIR` | leak-prone — macOS GUI apps inherit env, so `TERM_PROGRAM` outranks these, and Warp is checked before Ghostty |

TTY from `/usr/bin/tty` (fall back to `ps -p <ppid> -o tty=`). ~20 lines, no
new dependencies. `ponytail:` no AppleScript locators — tab-level targeting is
the §8 upgrade, not this step.

---

## 4 · The installer — additive, and it must negotiate

Built as pure functions in [install.rs](crates/sessions/src/install.rs):
`install` (append-only, matched by `aloud-hook` in the command), `uninstall`
(exact, collapses empty groups), `take_over` (the only evicting path, never
automatic), `permission_conflict`, `installed_events`. Atomic
temp-file+rename writes in `src-tauri`.

The installed shape is already the reference's: status events with no
timeout, `PermissionRequest` with `"timeout": 86400` — and the live
`~/.claude/settings.json` on this machine confirms it (aloud owns
`PermissionRequest` at 86400 today, coexisting with petdex's non-blocking
entries; Open Island's hooks are currently uninstalled). No `timeout` is
needed on `PreToolUse` once it stops blocking — the 60 s default is fine for
a hook answered in milliseconds.

The three standing rules, unchanged: never touch a foreign entry (Claude runs
every hook registered for an event, in order — coexistence is normal); the
blocking event is exclusive (foreign owner → offer **Take over** /
**Leave it**, never silent eviction); uninstall is exact and never runs on
quit (an agent mid-request would hang).

`install_agent_hooks` currently passes `approvals: false` while the live file
says approvals were installed at some point — reconcile: status reads the
file, not a memory, and the settings pane (§7) is where `approvals: true`
gets flipped deliberately.

---

## 5 · `src-tauri/src/sessions.rs` — wiring, and the fix list

Built: socket server (one thread per connection — a gate parks for a day and
must not queue behind status events), pending map + mpsc channel per request,
`agent-sessions` / `agent-permission` / `agent-permission-resolved` events,
change-only emission, `resolve_permission`, hook install/uninstall/status
commands, `tool_input` flattened to `BTreeMap<String,String>` for specta.

**The fix pass, in order.** Item 1 is the reason the feature doesn't work;
items 2–4 are one structural change (§1's "ingest hook events").

1. **Rebuild the approval path on the reference's model.**
   - `await_decision` must not require `tool_use_id`. **Key the pending map
     by `session_id`** — one live approval per session, a new one replaces a
     stale one. (Claude blocks the turn on the request, so one-per-session is
     the protocol's own invariant; concurrency across sessions — the swarm
     case — still works, and the UI keeps its queue of cards, one per asking
     session.)
   - Recover the display `tool_use_id`/input when wanted via the correlation
     cache: on `PreToolUse`, stash `tool_input` under
     `session_id|tool_name|canonical-json(tool_input)`; a following
     `PermissionRequest` matches on the same key. Display-only — routing
     never depends on it.
   - **Clear stale cards.** Any later hook event for a session with a pending
     interaction (`SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Stop`)
     means it was answered in the terminal: drop the pending entry, reply
     with no directive, emit `agent-permission-resolved`. Without this, the
     parallel terminal prompt strands a card on every terminal-side answer.
   - **Watch the socket while parked.** `rx.recv()` becomes a
     `rx.recv_timeout(5 s)` loop that probes the stream each lap (zero-byte
     read → `Ok(0)` = EOF); on EOF drop the pending entry and emit
     `agent-permission-resolved`. A hook that dies waiting (ctrl-C the agent)
     currently leaves the card up forever and leaks the thread. The
     reference's cleanup is EOF-driven; ours must be too.
   - Resolve optimistically in the UI (card comes down on click, before the
     socket write), as the reference does.
2. **One store** — hook events feed `SessionStore` (§1); `record()`'s
   parallel `hooked` map goes away, and with it the partial emit that blinks
   discovery rows out on every hook event, and the cwd-only merge that loses
   ai-title / running tool / subagents the moment hooks are installed.
3. **Hooked-session liveness** — the store's 2-poll grace reaps sessions
   whose process died without `SessionEnd`. Free once item 2 lands.
4. **Incremental scan** (§1) — noted here because the 3 s poll is the
   multiplier.

Kept as-is, deliberately: change-only emission (a 3 s no-op re-render
restarts the mascot's CSS animation), busy-first row sort (tested; the
reference orders by first-seen instead — revisit only if reshuffling grates),
0600 socket perms, stale-socket unlink before bind.

---

## 6 · The window and the UI — as built

The build put the agent surface **in the existing overlay window as tabs**
(`overlay.tsx`, with `agent-steps.ts` / `tool-views.tsx` in `src/lib`) rather
than the planned separate window. That stands — one window already welded to
the notch, one hit-region system, dictation suppression free. `ponytail:` the
split into a `notch` window is the upgrade path if `overlay.tsx` outgrows
comprehension; the recipe is in this file's git history.

**The tab system stays.** The strip across the panel top (`TabStrip`,
`MAX_TABS`, `+` for a blank tab) is a shipped feature, not scaffolding — tab 0
is the live ask/agent run, and extra tabs ride the same fixed-width body so
adding one never moves the panel's shoulders. Any panel rework keeps it.

**Cards mirror the terminal, verbatim.** The approval card offers the CLI's
own three options — **Yes** / **Yes, don't ask again for `<tool>` this
session** / **No, and tell Claude what to do differently (esc)** with a
message box — and an `AskUserQuestion` call renders as the model's own
question card: header chip, options with descriptions, `multiSelect`, an
injected freeform **Other**, one section per question, tap-to-answer when a
single single-select question. The wire shapes for all of it are in §2.

Still open:

- **Mascots** — not built. Sprite convention holds: N-frame horizontal strip
  at `src/assets/agents/<name>.webp`, `background-position` + `steps(N)`,
  `image-rendering: pixelated`, frame count declared beside the file. Port
  agent-notch's two-frame critter as the default, as a pure frames→cells
  function with a test.

Load `aloud-motion` before touching any of this. Walk cycles are CSS
keyframes; panel open/close stays on `spring.ts` (interruptible).

---

## 7 · Settings — not built

In [`crates/store/src/settings.rs`](crates/store/src/settings.rs):
`agent_notch_enabled` (default on), `agent_approvals_enabled` (default **off**
— it puts Aloud in another agent's execution path, which should be a
deliberate click; flipping it re-runs `install` with `approvals: true`), and
`agent_art`.

A pane in [`SettingsView.tsx`](apps/desktop/src/features/settings/SettingsView.tsx)
listing detected agents with per-agent Install / Uninstall, live hook status
(`agent_hook_status` already returns it), the Codex `/hooks` trust reminder,
and the conflict warning naming the foreign owner with **Take over** /
**Leave it** (`take_over` exists and is waiting on this UI).

---

## 8 · Jump back to the terminal — not built

Prerequisite: §3's enrichment (the terminal fields arrive empty today).

`ponytail:` **activate the app by bundle id; do not chase the tab.** Right
app, wrong tab is 80 % of the value for 5 % of the work.
`aloud_selection::installed_apps()` already resolves bundle ids. Tab-level
targeting — AppleScript for iTerm2/Terminal, a Warp SQLite read, tmux
`select-window`, `cursor://` and `codex://threads/<id>` deep links — is the
upgrade path and a project of its own.

While here: clear the row's "done" marker when a terminal or IDE takes focus —
reuse `aloud_selection::frontmost_app()` inside the existing poll rather than
adding an `NSWorkspace` observer.

---

## What this does not cover

- **Diff cards.** The reference has none either — a tool name and one clipped
  line; it even discards the `old_string`/`new_string` Cursor hands it. Our
  scanner already parses the `tool_use` blocks carrying `Edit`'s
  `old_string`/`new_string`, so a real before/after card is a cheap
  differentiator later — but it is later.
- **Usage / cost tracking** and the Claude status-line bridge.
- **Codex desktop app-server JSON-RPC** — real-time turn lifecycle for the
  desktop app, distinct from the CLI's hooks.
- **Watch / phone notifications.**
- **Tab-level jump-back** (§8) and the 15-terminal resolver behind it.
- **Gemini, OpenCode and the remaining forks** — see "Which agents".
- **`Notification` / `PermissionDenied` events** — parse and log, no UI.

---

## Traps — the original six, plus what the build taught

Standing: specta won't export `serde_json::Value` (flatten to `BTreeMap`,
keep the `Value` on the Rust side); the sidecar needs the target-triple name
and must be signed/notarized with the app; bindings only regenerate when run
from `apps/desktop/src-tauri`; concurrent requests across sessions are normal
(a card per asking session); quitting with a card up must EOF the hook into
fail-open (test by killing the app mid-card); `NSScreen`/window-level calls
are main-thread only.

Learned on this branch:

- **`PermissionRequest` carries no `tool_use_id`.** A handler that requires
  one silently drops every request, which is indistinguishable from the event
  never firing — and that misread cost this branch its approval path. Route
  by session id; recover the id for display via the `PreToolUse` correlation
  cache.
- **Do not gate on `PreToolUse`.** It fires on calls Claude would have
  auto-approved, so a gate there overrides the user's own permission modes
  and allowlists and freezes the terminal with no visible prompt.
  `PermissionRequest` fires only when Claude would ask — the filtering is the
  protocol's, for free.
- **A blocking hook entry needs an explicit `timeout`.** Claude kills a hook
  at ~60 s otherwise, and fail-open turns that into a silent auto-allow.
  (Moot for `PreToolUse` once it stops blocking; load-bearing on
  `PermissionRequest`, where the installer already writes 86400.)
- **Two session stores is one too many.** Every merge bug in §5 came from
  the hooked map living beside `SessionStore` instead of inside it.
- **A thread parked on a channel cannot see EOF.** Anything that waits on a
  human must also watch the socket it will answer on.

---

## Build order — remaining

1. **The approval rebuild + fix pass** (§5, all four items; §2's `wants_gate`
   reduction; §1's incremental scan). Ends when the verification list below
   passes — most of its ✗ lines fail today.
2. **Settings pane** (§7) — install/uninstall/conflict UI, approvals flipped
   on deliberately.
3. **Terminal enrichment + jump-back by bundle id** (§3 + §8).
4. **Codex end to end** — installer for `~/.codex`, verified against a real
   Codex CLI, swarm fold under the root thread.
5. **Mascots** (§6), "always allow", subagent disclosure polish.
6. Cursor and Gemini only if they are ever installed.

---

## Verifying

Unit tests do not prove OS integration, and this is nearly all OS
integration.

**Crates**

```sh
cargo test -p aloud-sessions -p aloud-bridge
cargo run -p aloud-sessions --example scan     # with real agents running
```

**The hook, before any UI** — and this time with the payload shape that broke
the branch, a `PermissionRequest` **without** `tool_use_id`:

```sh
echo '{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/x"}}' \
  | <data_dir>/bin/aloud-hook --source claude
```

With the app running it must raise a card and, on Approve, print the exact
allow directive. With the bridge down it must print **nothing**.

**Fail-open, explicitly.** Quit Aloud, run a real agent, confirm no added
latency and no hang on a tool call. This is the one whose failure breaks work
outside this app.

**App** (`pnpm dev`) — ✗ marks the lines that fail before the fix pass:

- A Claude Code session hosted in an IDE appears at all.
- ✗ **A gated command in default mode raises a card** (the payload has no
  `tool_use_id`; today it is silently dropped). Approve → it proceeds; Deny →
  the agent reports the denial.
- ✗ **Claude's own terminal prompt appears in parallel**, and answering
  *there* brings the card down by itself.
- ✗ **Approve after 10 minutes** — the request must still be open.
- ✗ **A session in `acceptEdits` editing a file shows no card**; one in
  `bypassPermissions` shows none at all; an allowlisted command shows none.
- ✗ **ctrl-C the agent with its card up** → the card comes down by itself.
- ✗ **`kill -9` a hooked session** → its row is gone within two polls.
- ✗ **While one hooked agent runs tools, an unhooked session's row must not
  flicker** — watch for a full poll interval.
- ✗ **With hooks installed, a row still shows its ai-title, running tool and
  subagents** (discovery enrichment reaching the hook session).
- Two agents in one project with hooks on → two rows (session ids differ).
- Install while another tool owns `PermissionRequest` → the conflict prompt,
  and that tool's hooks **still intact** afterwards. Check
  `~/.claude/settings.json` by hand.
- Uninstall → only our entries gone, every other consumer's untouched.
- Dictate mid-run → the agent surface yields, and returns on release.
- Click 200 px below and left of the indicator → the app underneath gets it.

**Workspace gate.** CI is unbilled and red regardless (see
[PROGRESS.md](PROGRESS.md)), so the local run is the gate:

```sh
cargo clippy --workspace --all-targets   # must be warning-free
cargo test --workspace
pnpm --filter @aloud/desktop typecheck
pnpm --filter @aloud/desktop test
```

⚠️ Windows: `crates/sessions` returns empty, no agent surface, no hooks
installed. Unexercised, like every other Windows path in this repo.
