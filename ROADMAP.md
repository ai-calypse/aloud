# Roadmap — voice-dict parity

Ordered by phase. Each phase is independently shippable; within a phase, tasks
are mostly independent and can be parallelised across branches.

Sizing is honest, not optimistic:
**S** = under an hour · **M** = half a day · **L** = a day or more · **XL** = multi-day project.

Every task states its **Scope** (what to touch) and its **Done** (what the
output looks like, and how it is verified). Per `CLAUDE.md`, anything touching
audio, keys, the clipboard or the screen ships an `examples/` binary and the
result goes in the PR body.

---

## Corrections to the parity audit

Two claims in the source audit are wrong against the code, and the plan below
reflects the code rather than the audit:

1. **`Binding::KeyCombo` does not exist.** `crates/hotkey/src/model.rs:65`
   defines `Disabled` / `Key { key, modifiers }` / `ModifierKey { key, modifiers }`.
   The variant unreachable from the UI is `Key`, whose `Key` enum
   (`model.rs:18`) already covers `Char(char)`, `F(u8)`, `Escape`, `Space`.
   The recorder task stands; the type name in the audit does not.

2. **Audio is never persisted.** The audit treats a Run Log audio player as
   part of one pane. WAVs are temp files (`pipeline.rs:893`) deleted
   immediately after transcription (`pipeline.rs:379`), and `history.rs` has no
   path or blob column. Playback is a storage-design change, not a UI change,
   so it is split out as Phase 5.

A third thing worth naming before any work starts: **the updater cannot be
built as a UI task.** Aloud registers the plugin but has no signing keypair and
no release feed. Phase 8 is release infrastructure with a UI on the end, and
until the keypair exists every pixel of that surface is dead. Do not start it
from the frontend.

---

## Phase 0 — Corrections and free wins

Small, independent, no design decisions. Good first branch.

### 0.1 — Gemini API key field · S
- **Scope** — `apps/desktop/src/features/settings/SettingsView.tsx:376-397`
  (the `Group title="Keys"` at the bottom of the Connectors pane). The field
  already exists end to end: `crates/store/src/settings.rs:32`, env fallback at
  `:169`, TS type at `bindings.ts:307`.
- **Done** — a Gemini row sits alongside Cerebras and Composio, using the same
  `rowInput` from `components/ui/rows.tsx`. Setting it makes agent mode work
  without editing `settings.json` or exporting `GEMINI_API_KEY`.

### 0.2 — Fix the Cerebras/Gemini label · S
- **Scope** — same Keys group. The pane credits Cerebras with "Understands a
  spoken command"; `src-tauri/src/agent.rs:122-138` shows Gemini is what agent
  mode reaches for, with Cerebras as fallback.
- **Done** — Gemini carries the command-understanding caption; Cerebras is
  described as the fallback it actually is. Verify by reading `agent.rs`, not
  by trusting this file.

### 0.3 — Ungate the Timings pane · S
- **Scope** — `apps/desktop/src/App.tsx:46-48`, the `import.meta.env.DEV`
  spread. `recent_timings` is registered (`lib.rs:85`) and `TimingsView.tsx`
  is complete and unreachable in release.
- **Done** — decide deliberately: either move Timings behind a "Show developer
  tools" setting, or delete the pane. Shipping a finished view that no user can
  reach is the one outcome to avoid. Recommend the setting.

### 0.4 — Refresh the stale PROGRESS.md claims · S
- **Scope** — `PROGRESS.md:51` ("no way to re-run it") and `:63` ("menu bar
  beyond Open/Quit"). Both are false — the tray is built out
  (`src-tauri/src/tray.rs`) and has "Re-run Setup…".
- **Done** — the Not-done list matches reality, and picks up the items this
  roadmap confirms are genuinely missing.

### 0.5 — Split the App Rules two-mode radio · S
- **Scope** — `apps/desktop/src/features/settings/AppRuleEditor.tsx`. The model
  carries independent `post_processing_enabled` and `custom_prompt`
  (`crates/store/src/rules.rs`), but the UI collapses them into one radio, so
  "cleanup on, no style hint" is unreachable.
- **Done** — a toggle for cleanup and a separate optional prompt field. Every
  combination the model allows is now expressible. No Rust change.

---

## Phase 1 — Settings the backend already supports

Each is a `Settings` field plus one row. They share a migration-free path
because `#[serde(default)]` covers new fields (`settings.rs:21`).

### 1.1 — Clipboard toggles · S
- **Scope** — two new `Settings` fields; `crates/paste` currently does
  snapshot/restore unconditionally. Add `preserve_clipboard` and
  `transient_clipboard` (the latter marks the item so Raycast/Maccy skip it —
  macOS `org.nspasteboard.TransientType`, no Windows equivalent, so `#[cfg]`
  it per the platform-differences convention).
- **Done** — both toggles in the General pane; an `examples/` run in
  `crates/paste` showing the clipboard restored vs preserved, and the transient
  marking visible to a clipboard manager. PR body states the Windows path is
  a no-op.

### 1.2 — Shortcut start delay · S
- **Scope** — `Settings` field (0–500ms) consumed in `src-tauri/src/pipeline.rs`
  before recording starts. Slider in the Hotkeys pane.
- **Done** — a non-zero delay measurably postpones capture. Verify against the
  timings waterfall, which already records stage starts (`timings.rs:45`).

### 1.3 — Mute system audio during dictation · M
- **Scope** — new, platform-owned. macOS has no public output-mute API worth
  relying on; the honest implementation is `osascript -e "set volume …"`
  saved/restored around the session, or CoreAudio via `coreaudio-sys`. Windows
  is `IAudioEndpointVolume`. Belongs in a crate, not `src-tauri`.
- **Done** — toggle in General; volume restored even if the app crashes
  mid-dictation (restore on next launch from a persisted flag). `examples/`
  binary that mutes, sleeps, restores.

### 1.4 — Show menu bar icon toggle · S
- **Scope** — `Settings` field read by `src-tauri/src/tray.rs`.
- **Done** — hiding the icon leaves hotkeys working. Include the escape hatch:
  if the icon is the only way into settings, hiding it must not strand the
  user — decide and document the recovery path (recommend: hiding is only
  offered once a hotkey to open settings exists).

### 1.5 — History retention control · S
- **Scope** — `history_retention` exists (`settings.rs:65`) and prunes on every
  insert (`history.rs:70`), with no UI.
- **Done** — a stepper or select in General. Natural companion to Phase 4.

### 1.6 — API key validation on save · M
- **Scope** — a `validate_key` command per provider doing one cheap authed call.
  Aloud currently saves blind, which is how an expired Groq key
  (`PROGRESS.md:25`) stays invisible until dictation fails.
- **Done** — inline ✓ / ✗ / spinner per key row. Must not block saving — a
  user offline still needs to paste a key. Validation is advisory.

---

## Phase 2 — Hotkey recorder

### 2.1 — Keystroke recorder · L
- **Scope** — `apps/desktop/src/features/settings/HotkeyEditor.tsx`. Today
  `CHOICES` (`:33`) offers Off plus six modifier-only holds built by `modKey()`
  (`:43`), and anything else falls to the `"custom"` sentinel rendering
  "Custom (edit file)" (`:100`). Needs a Record… / Done / Cancel flow that
  captures real keydowns and emits `Key { key, modifiers }` as well as
  `ModifierKey`. The capture must go through the existing event tap rather than
  the DOM — a browser keydown cannot see modifier-only presses or side-specific
  modifiers, which are the whole point of `crates/hotkey`.
- **Done** — every `Binding` the model can express is authorable from the UI:
  regular keys, modifier-only, and combos. Conflicts between roles are detected
  (`matcher.rs` orders by `specificity()` at `model.rs:103` — surface that).
  Verified with a real `CGEvent`, never AppleScript (`CLAUDE.md` notes
  synthesized keys arrive with keycode 0).

### 2.2 — Command/Edit Mode settings · M
- **Scope** — command mode ships without an enable toggle, an
  Automatic-vs-Manual invocation picker, or an extra-modifier picker
  (`PROGRESS.md:49` admits this). Fields go in `Settings`, UI in the Hotkeys
  pane next to 2.1.
- **Done** — command mode can be turned off, and its invocation style chosen,
  without editing JSON. Depends on 2.1 for the modifier picker.

---

## Phase 3 — Prompt and vocabulary tooling

### 3.1 — Test System Prompt runner · M
- **Scope** — Prompts pane. A sample-input field, a Run button hitting the
  existing cleanup path in `crates/llm`, and a result panel.
- **Done** — editing the prompt and running it shows the model's output without
  a dictation round trip. Rate limits and errors surface as text, not a toast
  that vanishes.

### 3.2 — Newer-default-prompt banner · M
- **Scope** — prompts are byte-for-byte `.md` files (`PROGRESS.md:89`). Compare
  the stored `custom_system_prompt` against the shipped default and, when the
  default has moved since the user forked it, offer View Default / Switch.
- **Done** — needs a stored "default version the user last saw"; without it the
  banner either never fires or never dismisses. That field is the actual work.

### 3.3 — Add clipboard word to vocabulary · S
- **Scope** — tray menu item (`src-tauri/src/tray.rs`), appending clipboard
  contents to `Settings::vocabulary` (`settings.rs:48`), deduped.
- **Done** — menu item works and flashes a checkmark on the tray icon. The
  flash is the fiddly part — the menu rebuilds wholesale on phase change
  (`tray.rs:35-50`), so the transient state needs to survive a rebuild.

### 3.4 — Output language / translation · M
- **Scope** — `Settings` gains a target language; `crates/transcribe` and the
  cleanup prompt both need it. Whisper's `translate` task only ever targets
  English, so anything else is a post-processing instruction, not a
  transcription parameter. Decide which, and say so in a comment.
- **Done** — picker in General; a non-English target visibly translates. Verify
  end to end, not by unit test.

---

## Phase 4 — Run Log pane (text)

### 4.1 — Per-run rows · L
- **Scope** — a new settings section (`SettingsView.tsx:29-35` union) backed by
  `history.rs:90` `recent()`. Rows show `created_at`, `app_name`,
  `duration_ms`, `final_transcript`. Actions: copy, delete, export.
- **Done** — replaces the tray's last-10 as the primary surface. Tray stays for
  quick copy. Deletion needs a `delete(id)` on `History` — currently only
  `clear()` exists (`history.rs:120`).

### 4.2 — Expanded pipeline breakdown · L
- **Scope** — join `history` rows to `timings` spans by `utterance_id`
  (`timings.rs:45`). Each stage shows duration, success, and the `fields` JSON
  blob it already records. `raw_transcript` vs `final_transcript` is the
  post-processing before/after, already stored.
- **Done** — expanding a run shows context capture → transcription →
  post-processing with each stage's real output. Blocked on one thing:
  `history.Entry` has no `utterance_id` (`history.rs:17-31`), so the join key
  must be added first. That is the first commit of this task.

### 4.3 — Retry transcription · M
- **Scope** — re-run cleanup on a stored `raw_transcript`. Cheap, because it
  needs no audio.
- **Done** — retry produces a new `final_transcript` in place. Re-running
  *transcription* (not cleanup) needs Phase 5 and is out of scope here.

### 4.4 — Vocabulary chips per run · S
- **Scope** — record which vocabulary entries were in play. Needs a field on
  the history row or a timings span.
- **Done** — expanded view lists them. Do this while 4.2 is adding a column.

---

## Phase 5 — Audio persistence (unlocks Run Log playback)

### 5.1 — Persist recordings with a retention budget · L
- **Scope** — the real work behind "an audio player in the Run Log". Touches:
  drop the `remove_file` at `pipeline.rs:379`; write into a media dir under
  `aloud_notes::data_dir()`; add a path column to `history` (schema is a bare
  `CREATE TABLE IF NOT EXISTS` at `history.rs:54`, and `crates/store/src/migrate.rs`
  is a Swift-app importer, **not** schema versioning — so schema migration
  machinery has to exist first); teach `prune()` (`history.rs:109`) to unlink
  files; add a size/age budget and its setting.
- **Done** — recordings survive, disk usage is bounded and visible, and pruning
  never orphans a file or a row. Verify by filling the budget and watching it
  evict. Compression (WAV → Opus) is the obvious follow-up, not part of this.

### 5.2 — Audio player in the expanded run · M
- **Scope** — trivial once 5.1 lands: a Tauri asset-protocol URL into an
  `<audio>` element.
- **Done** — playback from the Run Log. **Strictly blocked on 5.1.**

### 5.3 — Retry transcription from audio · S
- **Scope** — extends 4.3 to re-run the transcription stage.
- **Done** — a bad transcript can be re-run against a different engine or
  model. Blocked on 5.1.

---

## Phase 6 — Pipeline behaviour

### 6.1 — Instruction Guard · M
- **Scope** — detect when post-processing *answered* the dictation instead of
  cleaning it, then retry or fall back to the literal transcript. Lives in
  `crates/llm` next to the hallucination thresholds.
- **Done** — port the Swift heuristic verbatim per the port-intent convention,
  and say so in a comment. Regression tests named for the failure, using real
  captured examples of the bug. This is the task most likely to be wrong if
  re-derived from scratch — read the Swift source, do not invent a heuristic.

### 6.2 — Offline fallback · L
- **Scope** — network monitoring plus automatic fallback to local whisper when
  the cloud path fails. Aloud already has local whisper as an *engine choice*
  (`settings.rs:35`); this makes it a fallback. voice-dict uses Apple Speech;
  local whisper is the better Rust tool here and works on Windows.
- **Done** — pulling the network mid-dictation still produces text. Verified by
  actually disabling the interface, not by mocking. Must not silently fall back
  without telling the user which engine ran — surface it in the Run Log.

### 6.3 — Typed Ask · M
- **Scope** — the ask window is voice-only. Add a text input.
- **Done** — a question can be typed and answered without speaking. Mostly
  frontend; the ask path already accepts text downstream of transcription.

---

## Phase 7 — Overlay and display

### 7.1 — Overlay style choice · L
- **Scope** — minimalist menu-bar wings vs the current drop-down pill. Aloud's
  overlay is fixed bottom-centre by decision (`PROGRESS.md:96`), because no
  cross-platform API finds a notch. The wings style is macOS-only; scope it
  behind `#[cfg]` and accept that Windows keeps the pill.
- **Done** — both styles selectable on macOS, with the decision note in
  `PROGRESS.md` updated rather than contradicted.

### 7.2 — "Show on" display picker · M
- **Scope** — active window / primary / named monitor, refreshing as monitors
  come and go. Tauri exposes monitor enumeration.
- **Done** — the overlay follows the chosen display, and unplugging the chosen
  monitor falls back to primary instead of rendering off-screen. That fallback
  is the test.

### 7.3 — Permissions card · M
- **Scope** — per-permission status with Grant buttons. `hotkeys_permitted`
  exists (`commands.rs:118`); microphone, accessibility and (with Phase 9)
  screen recording need the same treatment.
- **Done** — every permission the app needs shows live status and a button that
  opens the right System Settings pane. macOS-only; Windows shows nothing.

### 7.4 — Build info click-to-copy · S
- **Scope** — version, commit, build date in the About/General pane.
- **Done** — clicking copies a paste-ready block for bug reports.

---

## Phase 8 — Updates (release infrastructure first)

**Do not start this from the UI.** The plugin is registered and inert because
there is no signing keypair and no release feed. Also note `PROGRESS.md:10` —
GitHub Actions is blocked on billing, so the feed has no publisher today.

### 8.1 — Signing keypair and release feed · L
- **Scope** — generate the updater keypair, wire the pubkey into
  `tauri.conf.json`, publish a signed feed. Depends on CI being unblocked, or
  on a documented manual release process.
- **Done** — a real update is detectable end to end from a real feed. Until
  this is true, nothing below can be tested.

### 8.2 — Update settings surface · M
- **Scope** — auto-check toggle, Check Now, last-checked date.
- **Done** — states are driven by the real updater, not mocked. Blocked on 8.1.

### 8.3 — Download progress, cancel, install, retry · M
- **Scope** — progress bar, Cancel, installing/relaunching states, retry on
  error.
- **Done** — every state reachable against a real feed, including failure.
  Blocked on 8.1.

### 8.4 — "What's New" release notes · S
- **Done** — release notes render from the feed. Blocked on 8.1.

### 8.5 — Post-dictation update chip · S
- **Scope** — an "Update Available" chip on the overlay.
- **Done** — appears after dictation when an update is pending, and never
  interrupts the paste. Blocked on 8.1.

---

## Phase 9 — Screen context (largest single project)

### 9.1 — Screenshot capture crate · XL
- **Scope** — a new crate capturing the frontmost app plus a screenshot.
  macOS needs the Screen Recording permission, which Aloud does not currently
  request — that is a TCC prompt, an Info.plist usage string, and a permission
  row (7.3). Windows needs its own path. `crates/selection` already reads the
  focused app and is the model for the platform split.
- **Done** — an `examples/` binary captures a screenshot and the frontmost app
  on real hardware. Permission denial is handled, not crashed on.

### 9.2 — Context in the cleanup prompt · M
- **Scope** — feed the captured context to post-processing in `crates/llm`.
- **Done** — dictation demonstrably reflects on-screen context. Blocked on 9.1.

### 9.3 — Context settings surface · M
- **Scope** — Context Prompt editor, screenshot-resolution picker, Test Context
  Prompt runner (reuse 3.1), screenshot-capture-failed alerts.
- **Done** — the whole context feature is configurable and testable from
  settings. Blocked on 9.1 and 9.2.

---

## Phase 10 — Realtime streaming

### 10.1 — Streaming transcription · XL
- **Scope** — changes the shape of the pipeline: transcription runs *while*
  recording rather than after. Touches `crates/audio` (streaming chunks),
  `crates/transcribe` (a streaming API), and `pipeline.rs`, which currently
  assumes stop → WAV → transcribe (`pipeline.rs:357`). A separate realtime
  model field in `Settings`.
- **Done** — text appears while speaking. This is a pipeline rewrite, not a
  toggle; sequence it after Phase 5, which also touches recording lifetime.

---

## Phase 11 — Surfacing what exists

### 11.1 — Meeting notes in the main window · M
- **Scope** — meeting notes are tray-only. No in-progress elapsed time or word
  count anywhere in the app window, despite `crates/meeting` having the data.
- **Done** — a pane showing an in-progress meeting and past ones.

### 11.2 — Onboarding expansion · L
- **Scope** — 4 steps vs voice-dict's 14. Missing: screen recording (blocked on
  9.1), all three shortcut-capture steps (blocked on 2.1), edit mode config
  (2.2), vocabulary, overlay style + mic pick (7.1), launch-at-login (the
  command exists at `commands.rs:135`), and a live test-transcription step.
- **Done** — a new user reaches a working dictation without opening settings.
  **Sequence this last** — it is a wrapper around features from Phases 2, 7 and
  9, and building it first means building it twice.

### 11.3 — App rules keyed on bundle ID · M
- **Scope** — Aloud matches on app name (`PROGRESS.md:53`); voice-dict keys on
  bundle ID, which survives renames and localisation.
- **Done** — rules key on bundle ID with a migration carrying existing
  name-based rules across. The migration is the work, not the matching.

### 11.4 — Debug pane · S
- **Scope** — overlay simulator, update-overlay preview. Pairs with 0.3's
  developer-tools setting; both live behind the same switch.
- **Done** — overlay states are inspectable without triggering real dictation.

---

## Suggested order

Phase 0 → 1 → 2 → 4 → 3 → 6 → 5 → 7 → 11.1/11.3 → 8 → 9 → 10 → 11.2

Rationale for the non-obvious moves:

- **4 before 5** — the text Run Log is most of the value and needs no storage
  redesign. Playback is a separate decision you can make after using the pane.
- **6 before 5** — Instruction Guard is a correctness fix affecting every
  dictation today; audio persistence is a convenience.
- **8, 9, 10 late** — each is gated on something other than code: a keypair, a
  TCC permission, a pipeline rewrite.
- **11.2 last** — onboarding wraps features that do not exist yet.

## Standing constraints

- Logic goes in a crate; `src-tauri` wires only.
- `src/lib/bindings.ts` is generated — never hand-edited.
- Colours go in `tokens.css`; check `components/ui/` before adding markup.
- New tokens must be added to the `cn()` class-group list in `lib/utils.ts`.
- Anything touching audio, keys, the clipboard or the screen ships an
  `examples/` binary, and the PR body says what was verified — and what was
  not. Every Windows path here remains unexercised while CI is blocked
  (`PROGRESS.md:10`).
