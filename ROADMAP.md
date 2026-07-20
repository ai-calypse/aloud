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

## Scope change: no BYOK

**Users do not supply API keys.** This is a production app, not a developer
tool, so every key entry surface comes out and the app ships with credentials
it obtains itself.

**Keys come from `.env` for now.** `fill_keys_from_env` already resolves every
key from a `.env` walked up from the working directory, so development needs no
input fields at all. That made the UI removal (**Phase A**, now complete)
unblocked and small. Provisioning for *shipped* builds is a separate, later
concern — **Phase 12** — because `.env` does not survive bundling.

### What this removes

All of this landed in #66.

| Was | Now |
|---|---|
| 0.1 Gemini key field | ✅ reverted — shipped in #59, removed again in #66 |
| 1.6 API key validation on save | ✅ deleted — nothing to validate |
| Groq key row (General → Transcription) | ✅ removed |
| Gemini + Composio rows (Connectors → Keys) | ✅ removed; the whole Keys group went |
| Onboarding key step | ✅ removed — onboarding is now 3 steps, and ungated |
| README "Bringing Cerebras back" step 3 (add a UI row) | ✅ rewritten — a restored Cerebras key comes from `.env`, not a row |
| "add one in Settings" error messages | ✅ reworded — they pointed at a field that no longer exists |

### What this does *not* remove

**The `Settings` fields, and `fill_keys_from_env`.** They become the *only*
mechanism, not a fallback — a dev run reads the repo's `.env` and every key
resolves. Deleting the fields would break that. Only the inputs go.

---

## Phase A — Remove the key surfaces · ✅ **complete (#66)**

**Decided: keys come from `.env` for now.** `fill_keys_from_env` walks up from
the working directory, so running from the repo picks up the repo's `.env` and
every key resolves without a single input field. That is the whole mechanism
until there is a release to ship. Provisioning for *shipped* builds is
**Phase 12**, deliberately late.

### A.1 — Remove the key surfaces · S · ✅ **done**
- **Scope** — the table above: the Groq row in General → Transcription, the
  whole `Group title="Keys"` in Connectors (Gemini + Composio), and the
  onboarding key step, which was one of only four steps and also carried the
  `blocked` save-gate on an empty cloud key. Onboarding is now three steps and
  cannot be blocked.
- **Kept the Rust fields.** `Settings` still holds them and
  `fill_keys_from_env` still fills them; only the inputs went. Removing the
  fields would have broken the `.env` path this phase depends on.
- **Done** — no `type="password"` input remains anywhere in
  `apps/desktop/src`, and a dev run from the repo still transcribes on cloud
  because `.env` supplied the key.

### A.2 — Local as the guaranteed floor · S · ✅ **done**
- **Scope** — `build_transcriber` bailed with "cloud transcription needs an API
  key". `effective_engine()` (`pipeline.rs`) now degrades an unconfigured cloud
  engine to on-device instead of erroring.
- **Why it was one shared helper** — the local-model cache in `transcriber()`
  keys on the engine too. Keyed on the raw setting while the build path fell
  back, whisper would reload (~200ms) on every dictation down that path.
- **Done** — verified with `transcribe_local` on real hardware with every key
  env var unset: model loaded in 304ms, transcribed in 136ms, correct output.
- **Still open, deliberately** — a *rejected* key (401) does not fall back, as
  the key is non-empty so `effective_engine` keeps Cloud. The user's recovery is
  the Engine dropdown, and the error message says so. Automatic
  fallback-on-failure is **6.2**, not this phase.
- **Also dropped** — `is_configured`, which had no production caller and is
  meaningless once local is a guaranteed floor.

---

## Phase 12 — Cloud key provisioning (deferred)

**Not needed until there is a shipped build.** `.env` covers development
entirely; this phase exists for the day the app is handed to someone else.

Recording the constraint now so it is not rediscovered late: **`.env` does not
survive bundling.** `env_or_dotenv` walks up from `current_dir()`, and a `.app`
launched from Finder has a working directory of `/`. It will find nothing and
every key will be empty. Nothing about this is fixable by moving the file —
bundling it into `Resources/` just means the user reads it with Show Package
Contents.

### 12.1 — Decide how a shipped build gets credentials · **decision, then L**

1. **Compile keys into the binary** (`option_env!`). Simplest, and the one to
   be careful with: `strings` over the bundle recovers the key in seconds, and
   it is then billed to us. Obfuscation does not change that — the decryption
   key ships too. Defensible *only* with a hard spend cap, a dedicated
   provider account, and the assumption that it is public on release day.

2. **Proxy through a backend.** Keys never ship. The standard answer, and real
   infrastructure: a service, auth, per-user rate limiting, and an outage takes
   dictation down.

3. **Short-lived credentials fetched at runtime.** Keeps the audio path direct
   to the provider, so the service is not in the hot path and never sees
   transcript content.

(2) and (3) both need a backend and a notion of user identity, which the app
has none of. That prerequisite is larger than everything else left here.

**Because local whisper needs no key at all, a fourth option exists and is the
cheapest: ship local-only, and treat cloud as the feature that waits for the
backend.** A.2 is most of the work for that already.

- **Done** — a build with no `.env` transcribes, and `strings` over the bundle
  shows no provider key. Write the second half as a check, not a hope.

### 12.2 — Stop storing keys in plaintext · M
- **Scope** — `settings.json` holds keys in cleartext in the app data dir,
  deliberately, matching the Swift app. Defensible for a key that is the
  *user's own*; a different call for a credential we issue. If 12.1 lands on
  (3), the short-lived token belongs in the OS keychain.
- **Done** — no long-lived credential in a plaintext file. Skip entirely if
  12.1 lands on (2) — nothing sensitive reaches the client at all.

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

### 0.1 — ~~Gemini API key field~~ · **shipped in #59, reverted in #66**
Superseded by the no-BYOK decision before it was ever used. The row came out in
A.1. The Rust field stays — `fill_keys_from_env` still needs it for dev builds;
only the input went.

### 0.2 — ~~Fix the Cerebras/Gemini label~~ → **delete `cerebras_api_key`** · S
Relabelling was the wrong fix. `grep` finds exactly three references to
`cerebras_api_key` — the field, its default, its env var — and no consumer. It
was never the agent's fallback; the doc comment saying so was aspirational. The
field, its UI row and its env entry are deleted in PR #59.

Wiring Cerebras up as a real fallback is a *feature*, and should be designed on
its merits rather than back-filled to justify a stale comment.

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

## Phase 1 — Settings the backend already supports · ✅ **complete**

Each is a `Settings` field plus one row. They share a migration-free path
because `#[serde(default)]` covers new fields (`settings.rs:21`).

Two of them turned out not to be "a field plus a row" at all: 1.2 needed a
generation guard so a release inside the delay cannot be outrun by the start it
races, and 1.3 needed a new crate and a disk file so a crash cannot leave the
machine muted. 1.1's `transient_clipboard` half is still deferred — it needs
`objc2-app-kit`.

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

### 1.2 — Shortcut start delay · ~~S~~ **M** · ✅ **done**
- **The obvious implementation is a bug.** A sleep in `Pipeline::start` runs on
  the CGEventTap callback thread: `handle_hotkey` → `aloud_hotkey::listen`'s
  callback → invoked inside the tap at `crates/hotkey/src/macos.rs:194`.
  Blocking there stalls every keystroke on the machine, and macOS disables a tap
  that overruns its timeout (`kCGEventTapDisabledByTimeout`) — a 500ms delay
  could kill the hotkey listener outright. Tried and backed out in PR #60.
- **Scope** — reserve the recorder slot under the session lock, return from the
  tap immediately, and start capture from a spawned thread after the delay. The
  guard against a second press arriving mid-delay is the actual work.
- **Done** — verified with real `CGEvent` presses against a 400ms delay: a
  1500ms hold recorded 1125ms of audio (capture postponed, and timed from the
  mic opening rather than the key press), a 150ms tap and four rapid 120ms taps
  produced no utterance at all, and the tap was never disabled. The guard is a
  generation counter on `Session`: every stop bumps it, so a release inside the
  delay invalidates the start it is racing.

### 1.3 — Mute system audio during dictation · M · ✅ **done**
- **Scope** — new, platform-owned. macOS has no public output-mute API worth
  relying on; the honest implementation is `osascript -e "set volume …"`
  saved/restored around the session, or CoreAudio via `coreaudio-sys`. Windows
  is `IAudioEndpointVolume`. Belongs in a crate, not `src-tauri`.
- **Done** — `crates/volume`, via `osascript` as predicted. The pre-mute level
  is written to disk *before* muting, so a crash leaves a restorable file rather
  than a silent machine; `restore()` runs on every stop path and once at launch.
  Verified with `mute_restore` on real hardware, including the crash path:
  volume 69 → muted → 69. **Windows is a no-op** (`IAudioEndpointVolume` is the
  real path, unwritten while no Windows user can test it).

### 1.4 — Show menu bar icon toggle · S · ✅ **done**
- **Scope** — `Settings` field read by `src-tauri/src/tray.rs`.
- **Done** — hiding leaves hotkeys working, applied live rather than at next
  launch. **The escape hatch is `RunEvent::Reopen`, not a settings hotkey.**
  macOS turns re-opening a running app into `applicationShouldHandleReopen`,
  which Tauri surfaces; `lib.rs` shows the window on it. So the recovery path is
  "open Aloud again", it costs no new dependency and no new hotkey role, and it
  does not collide with the recorder in #61. A settings hotkey remains a
  reasonable *addition* once 2.1 lands, but it is no longer load-bearing.

### 1.5 — ~~History retention control~~ · **already done**
Wrong in the parity audit and wrong here. A "History kept" number input already
sits at the bottom of the General → Behaviour group. No work needed.

### 1.6 — ~~API key validation on save~~ · **deleted (no BYOK)**
There is no key entry to validate. The underlying problem — a dead credential
being invisible until dictation fails — does not go away though; it moves to
A.2, where the app must fall back to local rather than failing silently — and
which matters more now, because a user with no key field has no way to fix it.

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
  overlay is fixed top-centre of the focused monitor by decision
  (`PROGRESS.md:101`), because no cross-platform API finds a notch — it has
  never been bottom-centre. The wings style is macOS-only; scope it
  behind `#[cfg]` and accept that Windows keeps the pill.
- **Done** — both styles selectable on macOS, with the decision note in
  `PROGRESS.md` updated rather than contradicted.

### 7.2 — "Show on" display picker · M
- **Scope** — active window / primary / named monitor, refreshing as monitors
  come and go. Tauri exposes monitor enumeration. Note that `position_top_center`
  already implemented the active-window mode *and* the primary fallback below;
  what was missing was the setting, the picker, and matching by name.
- **Done** — the overlay follows the chosen display, and unplugging the chosen
  monitor falls back to primary instead of rendering off-screen. That fallback
  is the test.

### 7.3 — Permissions card · M
- **Scope** — per-permission status with Grant buttons. `hotkeys_permitted`
  exists (`commands.rs:118`); microphone and (with Phase 9) screen recording
  need the same treatment. Accessibility does not: `hotkeys_permitted` *is*
  `AXIsProcessTrusted`, so hotkeys and accessibility are one TCC bucket and one
  row. Input Monitoring is a genuinely separate bucket, and nothing needs it.
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
- **Scope** — **3 steps** vs voice-dict's 14. It was 4; the API key step went
  in A.1, so 3 is the real starting point. Missing: screen recording
  (blocked on 9.1), all
  three shortcut-capture steps (unblocked by 2.1, shipped in #61), edit mode
  config (2.2), vocabulary, overlay style + mic pick (7.1), launch-at-login (the
  command exists at `commands.rs:135`), and a live test-transcription step.
- **Note** — voice-dict's 14 steps included key entry too. Parity here means
  matching the *useful* steps, not the count.
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

## Status

| Phase | State |
|---|---|
| 0 — Corrections and free wins | ✅ **merged** (#59). 0.1 reverted by A.1 in #66 |
| 1.1 — Clipboard toggles | ✅ **merged** (#63) — `preserve_clipboard`. Transient marking deferred (needs `objc2-app-kit`) |
| 1.2 — Shortcut start delay | ✅ **done** — reserve-then-spawn, tap never blocks |
| 1.3 — Mute system audio | ✅ **done** — `crates/volume`; Windows is a no-op |
| 1.4 — Menu bar icon toggle | ✅ **done** — reopen event is the escape hatch |
| 1.5 — History retention | ✅ already existed |
| 1.6 — Key validation | ❌ deleted (no BYOK) |
| 2.1 — Hotkey recorder | ✅ PR #61, open |
| 4.1 — Run Log pane | ✅ PR #62, open |
| 2.2 — Command mode settings | ◐ in progress |
| **A — Remove key surfaces** | ✅ **merged** (#66) — both A.1 and A.2 |
| **A.1 — Remove key surfaces** | ✅ no key input anywhere; keys come from `.env` |
| **A.2 — Local as the floor** | ✅ cloud without a key falls back to on-device |
| **12 — Cloud key provisioning** | deferred until there is a shipped build |
| everything else | not started |

⚠️ **CI has not run on any of this.** GitHub Actions is blocked on billing —
jobs report `steps=0` with no logs, meaning no runner is ever allocated. Every
merge so far landed on local verification only, and every Windows path in the
repo remains unexecuted.

Re-confirmed on #66: all three checks reported `FAILURE`, but each job had
`steps: []`, a 3-second duration, and `log not found`. That is a runner never
being allocated, not a test failing. **Check for that signature before reading a
red check here as a real failure — and before merging on the assumption that it
is not one.**

## Suggested order

~~Phase 0 → 1 → 2 → 4 → 3 → 6 → 5 → 7 → 11.1/11.3 → 8 → 9 → 10 → 11.2~~

Revised for no-BYOK:

**A.1 → A.2 → 2.2 → 3 → 6 → 5 → 7 → 11.1/11.3 → 8 → 9 → 10 → 11.2 → 12**

(0, 1, 2.1, 4.1 and A are done. Next up: 2.2, then Phase 3.)

Rationale for the non-obvious moves:

- **A is first and cheap** now that `.env` is the answer for development. It
  was previously blocked on a provisioning decision; it no longer is.
- **12 is last, and is the only item that can cost money if it is got wrong** —
  an embedded key in a shipped binary is an extractable key. Deferring it is
  fine precisely because nothing ships before it.
- **4 before 5** — the text Run Log is most of the value and needs no storage
  redesign. Playback is a separate decision you can make after using the pane.
- **6 before 5** — Instruction Guard is a correctness fix affecting every
  dictation today; audio persistence is a convenience.
- **8, 9, 10 late** — each is gated on something other than code: a keypair, a
  TCC permission, a pipeline rewrite. Phase 8 (updates) implies a release
  process, so it and Phase 12 land together or not at all.
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
