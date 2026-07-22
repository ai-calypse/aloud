# Agents in the notch

Notes toward showing coding agents in the notch — status while they work, a
session list, and approving what they ask to run without going back to the
terminal.

Nothing here is built. This exists so that when it is, the questions already
answered elsewhere are not answered again from scratch.

Aloud already owns the notch: [`notch.rs`](apps/desktop/src-tauri/src/notch.rs)
measures the real cutout, [`raise_over_menu_bar`](apps/desktop/src-tauri/src/overlay.rs#L72)
gets a window into the menu-bar strip, and
[`notch-shape.ts`](apps/desktop/src/lib/notch-shape.ts) +
[`spring.ts`](apps/desktop/src/lib/spring.ts) grow a panel out of it. What it
does not know is that Claude Code or Codex is running.

Two sources, neither one copied:

- **`../../agent-notch`** (MIT, ours) — read-only inference from `ps`/`lsof` and
  transcript tails. Zero config, and structurally incapable of showing a button.
- **`../open-vibe-island`** (**GPL v3 — do not copy code**) — a hook bridge.
  Approvals, ten agents, terminal jump-back, at the cost of an installer.

The hook contracts are each vendor's public protocol, not open-vibe-island's
invention — `PermissionRequest`, `PreToolUse`, `hookSpecificOutput` and
`permissionDecision` are all `strings`-visible in the shipped `claude` 2.1.215
binary. Building against those contracts is fine. Porting GPL Swift into this
MIT repo is not: write it fresh in Rust from the protocol, never from their
files.

**Two findings that drive the design:**

1. **Process discovery alone finds nothing on an IDE-hosted agent.** Claude Code
   run as the Cursor/VS Code extension has `tty = ??`, and agent-notch's first
   filter is `guard tty != "??"` (which exists to drop headless sessions). Hooks
   fire from inside the agent process and do not care where it lives.
2. **Other tools already hold hooks in `~/.claude/settings.json`.** On the
   development machine, Open Island owns `PermissionRequest` at a 24 h timeout
   while petdex shares `PreToolUse` / `Stop` / `UserPromptSubmit` — Claude Code
   runs all of them. So the installer must be *additive*, and permission
   ownership must be exclusive and negotiated. See §4.

---

## Which agents

Four payload dialects, not ten agents. Build per **format**.

| Format | Config it installs into | Approvals? | Agents covered |
|---|---|---|---|
| **Claude** | `~/.claude/settings.json` | ✅ `PermissionRequest`, 24 h | Claude Code, Qoder (`~/.qoder/`), Qwen (`~/.qwen/`), Factory (`~/.factory/`), CodeBuddy (`~/.codebuddy/`), Kimi (`~/.kimi/config.toml`, TOML `[[hooks]]`, same payload) |
| **Codex** | `~/.codex/hooks.json` + `[features] hooks = true` | ✅ `PermissionRequest`, 1 h | Codex CLI, Codex desktop |
| **Cursor** | `~/.cursor/hooks.json` | ✅ flat `{"permission":"allow"\|"deny"\|"ask"}` | Cursor's own composer agent |
| **Gemini** | `~/.gemini/settings.json` | ❌ fire-and-forget only | Gemini CLI |

One parser gets six agents. Cursor's event names are entirely different
(`beforeShellExecution`, `beforeMCPExecution`, `beforeReadFile`,
`afterFileEdit`), and Gemini has no permission flow at all
(`SessionStart`/`BeforeAgent`/`AfterAgent`/`Notification`), so those are
status-only or separate work. OpenCode is a JS plugin, not hooks — out of scope.

**Ship Claude format + Codex.** That covers everything installed here today, and
brings the four Claude forks along for free.

⚠️ Codex additionally requires a manual trust review — after installing, open
`/hooks` inside Codex CLI and approve the entries. That gate is Codex's and
cannot be bypassed. Say so in the UI, or the install silently does nothing.

---

## Architecture

```
agent process (any of the six)
  │  stdin: JSON payload
  ▼
aloud-hook            tiny Rust sidecar, spawned per event, ~ms
  │  Unix socket: <data_dir>/bridge.sock
  ▼
crates/bridge  →  src-tauri/src/sessions.rs  →  notch window
  │  decision
  ▼
aloud-hook
  │  stdout: JSON directive
  ▼
agent obeys
```

Plus a **fallback layer** — `ps`/`lsof` and transcript reading — for agents with
no hooks installed. Hooks are the truth where present; discovery fills the gaps.

---

## 1 · `crates/sessions` — the model and the fallback

No Tauri dependency. macOS body behind `#[cfg]`; Windows returns empty, the way
`notch.rs` does.

```
crates/sessions/
  src/lib.rs        Session, SessionStore (merges hook events + discovery)
  src/discovery.rs  ps + lsof → live transcript paths        [fallback]
  src/scan.rs       transcript tail reading                  [fallback]
  examples/scan.rs  one discovery+scan cycle, printed
```

The fallback ports these findings from `../../agent-notch`, each worth carrying
its comment:

- A session is a running agent process attached to a TTY. `ps -Ao
  pid=,ppid=,tty=,command=`. **Relax the `tty != "??"` guard** to also accept the
  IDE-extension binary path, or every IDE-hosted session reads as absent.
- **One batched `lsof -a -p <all pids> -Fn` per poll**, split on its `p<pid>`
  markers. Per-process spawns blow their timeout under load (a Codex swarm
  compiling) and every agent reads as dead.
- Claude Code appends-and-closes its transcript, so `lsof` usually shows none —
  fall back to the process `cwd`, one session per tty. Codex holds its rollout fd
  open, so the path route always works there.
- A process must be missing for **2 consecutive polls** (~6 s) before its session
  stops being live.
- Busy = alive **and** last *conversational* entry < 30 s old. `system` entries
  (hook summaries, compaction notes) bump the file mtime and must not count.
- Codex: head-read the first line for `parent_thread_id` and group swarms under
  the root; liveness observed on any member marks the whole group live, because
  one process serves the group but holds a single fd.

Two upgrades over agent-notch's reader, both confirmed against real transcripts:

- `{"type":"ai-title","aiTitle":"…"}` and `{"type":"last-prompt","lastPrompt":"…"}`
  exist as their own records — better than the folder name, and no backwards scan
  for the prompt.
- **A `tool_use` with no matching `tool_result` at the tail is what the agent is
  doing right now.** The line is written *before* the tool executes (verify by
  tailing a live transcript mid-call), so the panel can say
  `Bash: cargo test --workspace` rather than just "busy".

`ponytail:` the two subprocesses per poll (`libproc` is the upgrade path) and the
whole-tree `~/.codex/sessions` walk (mtime-filtered before any read).

**Tests**, each named for the failure it catches:
`housekeeping_writes_do_not_count_as_activity`,
`a_dead_process_survives_one_missed_poll`,
`codex_subagents_fold_under_their_root_thread`,
`orphan_tool_use_is_what_it_is_doing_now`, `lsof_chunks_split_on_pid_markers`.

---

## 2 · `crates/bridge` — the socket, both ends

Shared by the app and the sidecar so the wire format cannot drift.

- Socket at `<aloud_notes::data_dir()>/bridge.sock`. **Stable path, not pid-keyed**
  — a hook process has no way to know the app's pid. `unlink()` before `bind()`,
  `chmod 0600` after, per the notes already in [COMPUTER_USE.md](COMPUTER_USE.md).
- Payload types: `ClaudePayload` (also serves the four forks and Kimi) and
  `CodexPayload`. Fields: `session_id`, `transcript_path`, `cwd`, `model`,
  `permission_mode`, `tool_name`, `tool_input`, `tool_use_id`,
  `permission_suggestions`, `prompt`, `last_assistant_message`, plus
  `terminal_app` / `terminal_tty` / `terminal_session_id` / `terminal_title` —
  that last group is what makes jump-back possible (§8).
- Directive encoders, one per format. Claude allow:

  ```json
  {"continue":true,"suppressOutput":true,
   "hookSpecificOutput":{"hookEventName":"PermissionRequest",
     "decision":{"behavior":"allow"}}}
  ```

  Deny takes `{"behavior":"deny","message":"…","interrupt":false}`, and
  `interrupt:true` ends the turn outright. `PreToolUse` uses the older shape
  (`permissionDecision`: `allow`/`deny`/`ask`, plus `permissionDecisionReason`).
  Cursor, if it ever lands, is flat: `{"continue":true,"permission":"allow"}`.

Round-trip tests on every encoder. A typo in one of these key names is a silently
ignored directive, which looks exactly like the feature not working.

---

## 3 · `crates/hook-cli` → the `aloud-hook` binary

Deliberately tiny and dependency-light: it is spawned on **every tool call of
every agent**, so it has no business linking tokio.

1. `$ALOUD_SKIP_HOOKS=1` set → exit immediately, before reading stdin. Lets
   another controller own one run without changing global install state; it is
   the first thing wanted when two of these fight.
2. Read stdin to EOF. Empty → exit.
3. Connect to the socket with a **short connect timeout (~200 ms)**, then send.
4. Read timeout: **86 400 s for `PermissionRequest`, 45 s otherwise.** The long
   block is the whole trick — the agent is waiting on this process's stdout,
   which is what holds the decision point open.
5. Write the directive to stdout. No directive needed → write nothing.
6. **Fail open, always.** Socket missing, app not running, malformed reply,
   panic: exit silently with empty stdout and the agent proceeds as though
   nothing were installed. The 200 ms connect timeout is what stops a dead Aloud
   from adding latency to every tool call in every terminal on the machine.

**Installed to `<data_dir>/bin/aloud-hook`, not run from inside the `.app`.**
Bundled as a Tauri `externalBin` sidecar and copied out on install — a path
inside the bundle breaks the moment the app is moved or updated, while
`settings.json` keeps pointing at it. Re-copy and repair the recorded path on
launch when the version differs.

---

## 4 · The installer — additive, and it must negotiate

`crates/sessions/src/install.rs`, one module per format.

Claude format, events worth taking: `SessionStart`, `SessionEnd`,
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStart`,
`SubagentStop`, and `PermissionRequest` with `"timeout": 86400`. Codex:
`SessionStart`, `UserPromptSubmit`, `PermissionRequest` (`timeout` 3600), `Stop`,
plus flipping `[features] hooks = true` (older builds: `codex_hooks`).

Three rules, all learned from a machine that already has two other hook consumers
installed:

- **Never remove or rewrite a hook entry that is not ours.** Claude Code runs
  every registered hook for an event, in order, so coexistence is the normal
  case. Append only, and match our own entries by command path on re-install.
- **`PermissionRequest` is exclusive.** Two blocking consumers means two 24 h
  blocks and two competing directives. On install, detect a foreign
  `PermissionRequest` hook and stop — offer **Take over** / **Leave it**, where
  taking over disables approvals in Aloud or sets `ALOUD_SKIP_HOOKS`, but never
  silently evicts another tool's entry.
- **Uninstall must be exact and complete.** A manifest at
  `<data_dir>/hooks-install.json` (command path + timestamp) is how our own
  entries are found again after the binary path changes. Removing hooks on quit
  is wrong: an agent mid-`PermissionRequest` would hang.

Pure functions over config *contents* — `install_settings_json(existing, cmd) ->
Mutation` — so the whole installer tests without touching `~`. Kimi's TOML wants
a line-oriented editor, not parse-and-reserialize, which would discard the user's
comments.

---

## 5 · `src-tauri/src/sessions.rs` — wiring only

- Start the bridge server in `setup`; hold pending permission requests in a map
  keyed by `tool_use_id`, each with its reply channel.
- 3 s discovery poll for un-hooked sessions in `spawn_blocking` (it shells out),
  merged into the same `SessionStore`. Hook data wins on conflict.
- Event `agent-sessions` → the notch window, emitted only on real change: a 3 s
  no-op re-render restarts the mascot's CSS animation.
- Event `agent-permission` → one pending request, with agent, tool, input, cwd.
- Commands: `resolve_permission(tool_use_id, allow)`, `toggle_agent_panel()`,
  `set_agent_hit_boxes(boxes)`, `install_agent_hooks(agent)`,
  `uninstall_agent_hooks(agent)`, `agent_hook_status()`.
- Clear the "done" marker when a terminal or IDE takes focus — reuse
  `aloud_selection::frontmost_app()` inside the existing poll rather than adding
  an `NSWorkspace` observer. Bundle ids: Ghostty, Terminal, iTerm2, kitty, Warp,
  Alacritty, **Cursor, VS Code**.
- `suppress(app, bool)` hides the notch window while the recording overlay is up,
  called from `overlay::show` / `show_panel` and released in `hide` /
  `end_panel`. Dictation wins the strip.

Register the module, its commands and its events in
[`lib.rs`](apps/desktop/src-tauri/src/lib.rs).

---

## 6 · The window and the UI

A new `notch` window alongside `overlay`, same recipe: transparent, no
decorations, `alwaysOnTop`, `skipTaskbar`, `focus: false`, and one fixed 700×420
canvas that draws whatever it needs. Never resize a window per state.

Also required: `"notch"` in `capabilities/default.json`; a `notch` entry in
`vite.config.ts`'s `rollupOptions.input` (the comment there says what happens
otherwise); `notch.html` with `data-surface="floating"`; and in `setup`,
`raise_over_menu_bar` (generalized to take a label),
`set_ignore_cursor_events(true)`, `set_visible_on_all_workspaces(true)`,
`position_top_center`.

**Hit region.** Generalize [`point_in_panel`](apps/desktop/src-tauri/src/overlay.rs#L273)
to test a *list* of boxes reported by the webview: the collapsed indicator is a
small off-centre rect, not a panel centred and welded to the top. Keep
`only_the_panel_itself_takes_clicks` and add an off-centre case.

**`src/notch.tsx`** is a new root. Do not grow `overlay.tsx`, which is already
the dictation pill, the Ask answer and the agent run.

- *Collapsed:* one glyph slot per agent, right edge ~36 px left of the cutout.
  Mascot walking while busy, nothing while alive-but-quiet, a done marker when
  the process exits, cleared on terminal/IDE focus.
- *Expanded:* the session list, grown out of the notch with `notchPath` +
  `createSpring` so it reads as the same app as the Ask panel. Per row: agent,
  the user's own latest prompt (or the `ai-title`), what it is doing right now
  from the orphan `tool_use`, and a `model · 4m` tag. Subagents under a `▸ N`
  disclosure.
- *Permission card:* the panel opens itself on `agent-permission` and shows tool,
  input and cwd with Approve / Deny. **Reuse
  [`confirmation.tsx`](apps/desktop/src/components/ai-elements/confirmation.tsx)**
  — the same UI and the same question already exist here, pointed at Composio.

**Mascots: a sprite convention, not a hardcoded sheet.** An N-frame horizontal
strip at `src/assets/agents/<name>.webp`, animated with `background-position` +
`steps(N)` and `image-rendering: pixelated`, with frame count and cell size
declared beside the file. Any art drops in and walks; a settings dropdown picks
it per agent. The two-frame block-character critter at
`../../agent-notch/main.swift:667-691` is a fine built-in default — port it to
`src/lib/mascot.ts` as a pure frames→cells function with a test, because an
off-by-one there renders as confetti.

Load `aloud-motion` before writing any of this. The walk cycle is a CSS keyframe
(fixed and repeating, so a baked curve is right); the panel open/close is a
`spring.ts` spring (interruptible, because the user can click again mid-flight).

---

## 7 · Settings

In [`crates/store/src/settings.rs`](crates/store/src/settings.rs):
`agent_notch_enabled` (default on), `agent_approvals_enabled` (default **off** —
it puts Aloud in another agent's execution path, which should be a deliberate
click), and `agent_art`.

A pane in [`SettingsView.tsx`](apps/desktop/src/features/settings/SettingsView.tsx)
listing detected agents with per-agent Install / Uninstall, live hook status, the
Codex `/hooks` trust reminder, and the conflict warning when another tool owns
approvals.

---

## 8 · Jump back to the terminal

The hook payload already carries `terminal_app`, `terminal_tty`,
`terminal_session_id` and `terminal_title`, so this is nearly free, and it is the
one open-vibe-island feature that earns its keep daily: click a row, land in the
session.

`ponytail:` **activate the app by bundle id; do not chase the tab.** Right app,
wrong tab is 80% of the value for 5% of the work. Per-terminal tab targeting —
AppleScript for iTerm2/Terminal, a Warp SQLite read, tmux `select-window`,
`cursor://` and `codex://threads/<id>` deep links — is the upgrade path and a
project of its own. `aloud_selection::installed_apps()` already resolves bundle
ids.

---

## What this does not cover

Named so each is a decision rather than a surprise:

- **Usage / cost tracking** and the Claude status-line bridge.
- **Codex desktop app-server JSON-RPC** — real-time turn lifecycle for the
  desktop app, distinct from the CLI's hooks.
- **Watch / phone notifications.**
- **Tab-level jump-back** (§8) and the 15-terminal resolver behind it.
- **Gemini, OpenCode and the remaining forks** — see "Which agents".
- **`Notification` / `PermissionDenied` events** — installed and logged, driving
  no UI initially.

Everything `../../agent-notch` does *is* covered, including the fullscreen-space
case: when the menu bar is hidden the strip is ours and the indicator keeps its
position — test `visibleFrame.maxY >= frame.maxY - 1` and do not reposition.

---

## Traps this repo already knows about

Each is a day lost if it is discovered at implementation time.

- **specta will not export `serde_json::Value`, and `tool_input` is arbitrary
  JSON.** [`agent.rs:69`](apps/desktop/src-tauri/src/agent.rs#L69) hit this exact
  wall and solved it: reuse `display_args()` to flatten to
  `BTreeMap<String, String>` for the binding, and keep the real `Value` on the
  Rust side for the directive.
- **No `externalBin` exists in this repo yet.** Tauri wants the sidecar named
  with a target-triple suffix (`aloud-hook-aarch64-apple-darwin`), so a build
  step has to rename the `cargo build -p aloud-hook` output — and the second
  binary must be **signed and notarized** with the app or Gatekeeper kills it on
  anyone else's machine.
- **Bindings are emitted relative to the crate dir**, so the new commands only
  appear when the app runs from `apps/desktop/src-tauri`. `pnpm dev` handles it.
- **Concurrent requests are normal**, not an edge case — a Codex swarm can ask
  three times at once. Key the pending map by `tool_use_id` and render a queue,
  newest on top, rather than assuming a single card.
- **Quitting with a request pending must not hang an agent.** Closing the socket
  gives the hook an EOF, which has to take the same path as never connecting:
  fail open, print nothing. Test it by killing the app with a card on screen.
- **`NSScreen` and window-level calls are main-thread only** — the trap
  `crates/hotkey` and the notch measurement already hit. See
  [CLAUDE.md](CLAUDE.md) on `relax-sign-encoding`.

---

## Build order

Each step is useful on its own; nothing below needs the step after it.

1. **`crates/sessions` fallback + `examples/scan.rs`.** No UI, no sockets. Ends
   when it prints the machine's real sessions, including the IDE-hosted ones the
   TTY guard used to drop.
2. **`crates/bridge` + `aloud-hook` + the Claude installer.** Verified by piping
   a payload into the binary against a running bridge, before any UI exists.
3. **Window + indicator**, hook events driving it, dictation suppression.
4. **The panel**, then the permission card (a queue, not a single card).
5. **Codex format**, then jump-back by bundle id.
6. Cursor and Gemini only if they are ever installed.

Steps 1–4 are the whole product for Claude Code. Step 5 adds the second agent and
the daily convenience.

---

## Verifying

Unit tests do not prove OS integration, and this is nearly all OS integration.

**Crates**

```sh
cargo test -p aloud-sessions -p aloud-bridge
cargo run -p aloud-sessions --example scan     # with real agents running
```

Run the example three times: mid-turn (`live=true busy=true`), 60 s after the
turn ends (`live=true busy=false`), and after quitting the process (gone within
two polls).

**The hook, before any UI** — the step that catches wire-format typos:

```sh
echo '{"hook_event_name":"PermissionRequest","tool_name":"Bash", … }' \
  | <data_dir>/bin/aloud-hook --source claude
```

It must print the exact directive, and print **nothing** with the bridge down.

**Fail-open, explicitly.** Quit Aloud, run a real agent, and confirm no added
latency and no hang on a tool call. This is the one whose failure breaks work
outside this app.

**App** (`pnpm dev`)

- A Claude Code session hosted in an IDE appears at all.
- Ask it to run something gated → card in the notch; Approve → it proceeds; Deny
  → the agent reports the denial.
- Approve after 10 minutes — the 24 h block must still be open.
- A Codex swarm shows its subagents folded under one root.
- Install while another tool owns `PermissionRequest` → the conflict prompt, and
  that tool's hooks **still intact** afterwards. Check `~/.claude/settings.json`
  by hand.
- Uninstall → only our entries gone, every other consumer's untouched.
- Dictate mid-run → the indicator hides, the wings weld to the notch, and it
  returns on release.
- Click 200 px below and left of the indicator → the app underneath gets it.

**Workspace gate.** CI is unbilled and red regardless (see
[PROGRESS.md](PROGRESS.md)), so the local run is the gate:

```sh
cargo clippy --workspace --all-targets   # must be warning-free
cargo test --workspace
pnpm --filter @aloud/desktop typecheck
pnpm --filter @aloud/desktop test
```

⚠️ Windows: `crates/sessions` returns empty, no notch window, no hooks installed.
Unexercised, like every other Windows path in this repo.
