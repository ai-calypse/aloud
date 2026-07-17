# Morning Report — 17 July 2026

**The app works.** Hold a key, speak, release, and clean text lands in whatever
app has focus — cross-platform Rust and TypeScript, no Swift.

Verified on real hardware, not just in tests. Straight out of the history
table after a genuine key-hold with speech into the real microphone:

> `"Fox jumps over the lazy dog, testing a loud transcription."`

Hotkey → mic → local whisper → paste → history in **~620 ms**, fully offline.

Repo: <https://github.com/juligandhi-a11y/Aloud> · working clone at
`~/Documents/Development/apps/aloud-tauri`. Your Swift app is untouched.

---

## Two things need you

### 1. CI cannot be pushed — this is the important one

`ci/ci.yml` holds the full macOS + Windows CI matrix, but GitHub rejected the
push: the `gh` token lacks the `workflow` scope. I did not work around it.

**Why it matters more than a normal chore:** every Windows code path in this
repo — the `WH_KEYBOARD_LL` keyboard hook, WASAPI audio capture, `Ctrl+V`
paste — was written against documentation and **has never executed**. This
machine is a Mac. That CI run is the first thing that would prove any of it.

```sh
gh auth refresh -s workflow                      # grant the scope
mkdir -p .github/workflows && git mv ci/ci.yml .github/workflows/ci.yml
git commit -m "ci: build and test on macOS and Windows" && git push
```

### 2. Your Groq API key is expired

Migration imported it correctly, but live calls return
`401 Invalid API Key`. Local whisper is unaffected — it needs no key — so the
app works regardless. Cloud transcription and LLM cleanup will not until the
key is replaced in settings.

Silver lining: the 401 proved the cloud error path handles failure cleanly.

---

## Eight PRs to review

Each is self-contained, tested, and states what was verified and what was not.
Reviewing in order tells the story; each PR body has the detail.

| PR | What |
|----|------|
| [#1](https://github.com/juligandhi-a11y/Aloud/pull/1) | `audio` — mic capture, dual-rate output |
| [#2](https://github.com/juligandhi-a11y/Aloud/pull/2) | `hotkey` — hold/toggle/modifier-only bindings |
| [#3](https://github.com/juligandhi-a11y/Aloud/pull/3) | `paste` — clipboard snapshot/restore, synthetic paste |
| [#4](https://github.com/juligandhi-a11y/Aloud/pull/4) | `transcribe` — local whisper + cloud |
| [#5](https://github.com/juligandhi-a11y/Aloud/pull/5) | `llm` — chat client, prompts, rate limits, `CLAUDE.md` |
| [#6](https://github.com/juligandhi-a11y/Aloud/pull/6) | **the working dictation pipeline** |
| [#7](https://github.com/juligandhi-a11y/Aloud/pull/7) | migration from the Swift app |
| [#8](https://github.com/juligandhi-a11y/Aloud/pull/8) | the recording pill (screenshot in the PR) |

87 tests · `cargo clippy --workspace` zero warnings · `tsc --noEmit` clean.

---

## What shipped

```
crates/
  audio        cpal mic capture; 16 kHz WAV + 24 kHz realtime tap
  hotkey       CGEventTap (macOS) / WH_KEYBOARD_LL (Windows) + pure reducer
  llm          OpenAI-compatible chat, prompts, rate-limit breaker
  notes        markdown notes + search; app data paths
  paste        clipboard snapshot/restore, synthetic paste
  store        settings, history (SQLite), stats, migration
  transcribe   local whisper.cpp + cloud whisper
apps/desktop   Tauri v2 shell, React UI, recording overlay
```

Logic lives in crates with no Tauri dependency, so it is testable with
`cargo test` and runnable from `examples/` without launching the app.
`src-tauri` only wires them together. That is the deliberate opposite of the
4,000-line `AppState` god object it replaces.

---

## Bugs found — and why they matter

Every one of these **passed its unit tests** and was caught only by driving
real hardware. That is the pattern worth keeping.

**Empty WAV files.** `FftFixedIn` buffers internally and returns nothing for a
non-aligned buffer, so a 3-second recording wrote a 0-frame file. The tests
passed because 48k→24k happened to align; the real path is 48k→16k, which does
not. Now `FastFixedIn`, with regression tests on the exact failing lengths.

**Hotkey one-shots re-firing.** "Paste again" fired on every key repeat, since
one-shots were never recorded as active and each repeat looked like a fresh
press.

**The overlay rendering nothing.** The window reported `visible: true` at the
right coordinates and painted nothing. Logs were clean. A screenshot showed the
truth: the overlay window was missing from `capabilities/default.json`, so
Tauri v2 **silently denied its event listener** — no error, no warning. It
never learned its phase and stayed on `idle`, which is `display: none`.

**Silence claiming success.** Dictating silence reported "Pasted" when nothing
was pasted.

**whisper.cpp's stdout.** It logs from C, bypassing Rust. Installing its
logging hook took model load from **19.3 s → 204 ms**. The logging was the
bottleneck, not the model.

**The wrong data directory.** `ProjectDirs::data_dir()` yields
`app.Aloud.Aloud` on macOS. Your Swift app writes to a plain `Aloud` folder,
and migration reads from it — this would have silently targeted the wrong path.

---

## Decisions worth knowing about

**Tauri's `global-shortcut` plugin is unusable here**, so `crates/hotkey` is
hand-rolled. The plugin cannot express a modifier-only binding ("hold Right
Option"), cannot tell hold from tap, and cannot swallow the event before the
focused app sees it. All three are core to how this app dictates.

**Local whisper is the default.** It replaces `SFSpeechRecognizer`, which is
Apple-only, so offline dictation now works on Windows too — and the app is
useful on first launch with no key and no network.

**The prompts were copied byte-for-byte**, not retyped, and live as `.md`
files. Likewise the hallucination filter's thresholds: the Swift comment says
they were tuned on ~500 samples, so they were ported rather than re-derived.

**Clipboard preservation covers text and images only**, not the Swift version's
arbitrary `NSPasteboardItem` types, which have no cross-platform equivalent.

**The overlay uses CSS `backdrop-filter`**, not the Metal water-glass shader —
a visual downgrade, not a functional one. Placement is fixed bottom-centre;
there is no cross-platform way to find a notch.

**`macos-private-api` is enabled** for real window transparency. It rules out
Mac App Store distribution — fine for a notarized DMG, but a door closed
deliberately.

---

## Not done

Full parity was the goal and this is not it yet. What remains, roughly in
dependency order:

- **Ask mode** — the streaming client and SSE parsing are built and tested
  (PR #5); the floating answer panel and spoken triggers are not.
- **Command mode** — `copy_selection()` exists in `crates/paste`; the native
  AX/UIAutomation read and the transform flow are not wired.
- **Meeting notes** — `crates/audio` already emits the rolling PCM chunks it
  needs; the chunking loop and summary step are not written.
- **Notes window** — `search_notes` works end-to-end (it ranked your real
  imported notes correctly); no UI yet.
- **Settings UI** — the store and typed bindings are done; no forms.
- **Onboarding, realtime streaming, app context, Composio, voice macros,
  app rules, updater, signing/notarization.**
- **Agent Mode** — your mid-session request. Planned as Phase 9, after parity.

---

## If you only do three things

1. Grant the `workflow` scope and land CI. Windows is unproven until it runs.
2. Replace the Groq key.
3. Read PR #6 — that is the app actually working.
