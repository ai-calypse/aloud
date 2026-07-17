# Build Log

Running log of what's done, what's decided, and what's blocked.
See [MORNING-REPORT.md](MORNING-REPORT.md) for the summary; this is the detail.

---

## Blocked

**CI cannot be pushed.** `ci/ci.yml` holds the macOS + Windows matrix, but the
`gh` token lacks the `workflow` scope and GitHub rejects workflow files. Every
Windows path here (`WH_KEYBOARD_LL`, WASAPI, `Ctrl+V`) is written but never
executed — CI is what would prove it.

```sh
gh auth refresh -s workflow
mkdir -p .github/workflows && git mv ci/ci.yml .github/workflows/ci.yml
```

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
| Dictation pipeline | ✅ **verified end-to-end** |
| Migration from the Swift app | ✅ verified on real data |
| Recording overlay | ✅ verified by screenshot |

87 tests · clippy clean · typecheck clean.

## Not done

Ask panel · command mode · meeting notes · notes window · settings UI ·
onboarding · realtime streaming · app context · Composio · voice macros ·
app rules · updater · signing/notarization · Agent Mode (Phase 9).

---

## Architecture

```
crates/          logic, no Tauri dependency — testable with cargo test
apps/desktop/    Tauri v2 shell, React UI, overlay
  src-tauri/     wiring only
```

The deliberate opposite of the 4,000-line `AppState` this replaces.
See [CLAUDE.md](CLAUDE.md) for conventions and the traps found so far.

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

---

## Bugs found

Each passed its unit tests and was caught only by driving real hardware.

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
- **AppleScript-synthesized keys arrive with keycode 0**, so `osascript`
  cannot test the hotkey path. Verified by posting real `CGEvent`s instead.
