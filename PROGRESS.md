# Build Log

Running log of what's done, what's decided, and what's blocked.
See [MORNING-REPORT.md](MORNING-REPORT.md) for the summary; this is the detail.

---

## Blocked

**GitHub Actions is blocked on billing.** Every job fails within seconds with
*"recent account payments have failed or your spending limit needs to be
increased."* Nothing has run server-side since #34. Local checks are clean —
`cargo test --workspace`, `cargo clippy --workspace --all-targets`, `tsc`,
`oxlint`, `vitest`, `vite build` — but that is not the same as CI.

**CI still cannot be pushed.** `ci/ci.yml` holds the macOS + Windows matrix but
lives outside `.github/workflows/`, because the `gh` token lacks the `workflow`
scope. Every Windows path here (`WH_KEYBOARD_LL`, WASAPI, `Ctrl+V`) is written
and never executed; the matrix is what would prove it.

```sh
gh auth refresh -s workflow
mkdir -p .github/workflows && git mv ci/ci.yml .github/workflows/ci.yml
```

**No Composio API key on this machine.** Connector caching, the
stale-while-revalidate push and the disconnect flow are unit-tested and driven
against stubs, never against the live API.

**The Groq API key is expired** — live calls return 401. Local whisper is
unaffected; cloud transcription and LLM cleanup need a new key in settings.

---

## Done

| Area | State |
|---|---|
| Scaffold, tray, menu-bar-only | ✅ |
| `crates/audio` — mic capture | ✅ verified on real mic |
| `crates/hotkey` — global hotkeys | ✅ verified with real keycodes |
| `crates/paste` — clipboard + paste | ✅ verified at system level |
| `crates/transcribe` — local + cloud | ✅ local verified end-to-end |
| `crates/llm` — chat, prompts, limits | ✅ unit-tested; no live API |
| `crates/notes` — markdown + search | ✅ verified on real notes |
| `crates/store` — settings, history, stats | ✅ |
| `crates/agent` — tool loop, Composio, cache | ✅ unit-tested; no live API |
| Dictation pipeline | ✅ **verified end-to-end** |
| Migration from the Swift app | ✅ verified on real data |
| Recording overlay | ✅ level meter driven with synthesised levels |
| Notes window | ✅ verified in a headless browser against stubbed IPC |
| Settings UI | ✅ verified in a headless browser against stubbed IPC |
| Ask panel | ✅ |
| Command mode | ✅ core flow; no enable toggle or invocation style |
| Meeting notes | ✅ chunks transcribe serially on stop, not in the background |
| Onboarding | ✅ 4 steps, vs the Swift wizard's 14; no way to re-run it |
| Voice macros | ✅ inline expansion (the Swift feature matched whole utterances) |
| App rules | ✅ matched on app name, where Swift keyed on bundle ID |
| Agent Mode + Composio | ✅ tool loop lands; API keys have a settings pane |
| Frontend design system | ✅ tokens, primitives, motion — see below |

clippy clean · typecheck clean · 150 Rust tests · 14 frontend tests.

## Not done

Realtime streaming · app context/screenshot · updater (registered but inert —
no signing pubkey) · signing/notarization · network monitoring and the offline
fallback · typed ask · menu bar beyond Open/Quit · the Run Log settings pane.

---

## Architecture

```
crates/          logic, no Tauri dependency — testable with cargo test
apps/desktop/    Tauri v2 shell, React UI, overlay
  src-tauri/     wiring only
  src/styles/    design tokens — the single source of truth for the UI
  src/components/ui/  shared primitives; nothing app-specific below this line
```

The deliberate opposite of the 4,000-line `AppState` this replaces.
See [CLAUDE.md](CLAUDE.md) for conventions, the frontend architecture, and the
traps found so far.

---

## Decisions

- **Hand-rolled hotkeys.** Tauri's `global-shortcut` plugin cannot do
  modifier-only bindings, hold-vs-tap, or event swallowing. All three are core
  to dictation.
- **Local whisper is the default**, replacing the Apple-only
  `SFSpeechRecognizer`. Works on Windows, and on first launch with no key.
- **Prompts copied byte-for-byte**, not retyped, as `.md` files. Same for the
  hallucination thresholds — the Swift comment says they were tuned on ~500
  samples, so they were ported, not re-derived.
- **Clipboard preservation covers text and images only.** The Swift version's
  arbitrary `NSPasteboardItem` types have no cross-platform equivalent.
- **Overlay uses CSS `backdrop-filter`**, not the Metal shader; fixed
  bottom-centre, since no cross-platform API finds a notch.
- **`macos-private-api` enabled** for real transparency — rules out Mac App
  Store distribution, which this app does not use.
- **Dependency versions pinned from live crates.io**, not memory.
  `specta`/`tauri-specta` are exact-pinned RCs that move in lockstep.
- **One palette for all four windows**, scoped by `data-surface="floating"` on
  `<html>` rather than a second stylesheet. Declared in the markup so the
  palette is correct on the first paint rather than after React mounts.
- **Connector caching is stale-while-revalidate, not TTL.** OAuth finishes in
  the browser, so a cache that waits out a timer would hide a service connected
  seconds ago — wrong at exactly the moment it matters.
- **The installed-apps cache has a 5-minute TTL**, not process lifetime. This
  is a tray app that runs for weeks; a permanent cache means an app installed
  today never appears in App Rules.
- **Motion is CSS, driven by JS setting a target.** Not a rAF loop: the
  compositor should own every frame, so animations do not stutter when React is
  busy rendering the thing that is arriving.
- **Reduced motion collapses durations to 1ms** rather than `animation: none`,
  which can strand an element on its opening frame.

---

## Bugs found

Each passed its unit tests, or typechecked and built clean, and was caught only
by driving the real thing.

- **Empty WAVs.** `FftFixedIn` buffers internally and returns nothing for a
  non-aligned buffer. Tests passed because 48k→24k aligns; the real path,
  48k→16k, does not. Now `FastFixedIn`, with regression tests.
- **Hotkey one-shots re-firing** on every key repeat — they were never
  recorded as active, so each repeat looked like a fresh press.
- **Overlay rendering nothing** while reporting `visible: true`. The window was
  missing from `capabilities/default.json`, so Tauri v2 silently denied its
  event listener; it stayed on `idle`, which is `display: none`.
- **Silence reporting "Pasted"** when nothing was pasted.
- **whisper.cpp's C-side stdout**: installing its logging hook took model load
  from 19.3s → 204ms. The logging was the bottleneck, not the model.
- **`ProjectDirs::data_dir()`** yields `app.Aloud.Aloud` on macOS; the Swift
  app uses a plain `Aloud` folder that migration reads from.
- **AppleScript-synthesized keys arrive with keycode 0**, so `osascript` cannot
  test the hotkey path; a real `CGEvent` has to be posted instead.
- **App Rules froze the window rather than loading slowly.** `installed_apps`
  was a non-async Tauri command, and Tauri runs those on the main thread. The
  scan spawns `sips` once per bundle — 440ms for 71 apps, uncached.
- **Connectors was a double N+1** against Composio: one request per connected
  service for the toolkit, then another per service for its tools.
- **`dark:` never fired.** `@custom-variant` pointed it at a `.dark` class that
  nothing sets, so all 18 `dark:` utilities compiled to dead selectors. The
  palette only flipped because the CSS variables use the media query directly.
- **`var(--warning)` was undefined in the agent panel.** Only the main window
  imported the palette; the panel windows had their own partial copy.
- **Onboarding was invisible in light mode** — its stylesheet assumed a
  permanently dark backdrop while rendering in a window that follows the OS.
- **`tailwind-merge` silently dropped `text-brand`.** It groups by parsing the
  class name, and custom `text-ui` (a size) and `text-brand` (a colour) both
  look like `text-<unknown>`, so it kept only the last. Every Edit/Delete/Add
  link rendered in body-text black. Caught by reading computed styles.
- **The sliding pill never positioned on first paint** — its effect keyed on
  the active id, but the target element arrives from a callback ref one render
  later.
- **The listening bars were not listening.** Each ran a canned 0.9s sine loop;
  the measured level only squashed the whole row, so the motion was identical
  in speech and in silence.
