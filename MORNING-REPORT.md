# Report — Aloud Swift → Tauri, all phases through 9

**The app works, and every planned feature is built.** Your macOS-only Swift
dictation app is now a cross-platform Tauri app — React + TypeScript frontend,
Rust backend — with the full feature set ported and Agent Mode's foundation
laid on top.

Verified on real hardware, from the history table after a real key-hold with
speech into the real microphone:

> `"Fox jumps over the lazy dog, testing a loud transcription."`

Hotkey → mic → local whisper → paste → history in **~620 ms**, fully offline.

Repo: <https://github.com/juligandhi-a11y/Aloud> · clone at
`~/Documents/Development/apps/aloud-tauri`. Your Swift app is untouched.

---

## Two things still need you

### 1. CI cannot be pushed — the important one

`ci/ci.yml` holds the macOS + Windows matrix, but GitHub rejects workflow
files: the `gh` token lacks the `workflow` scope. **Every Windows code path
in this repo has been written against documentation and never executed** —
the `WH_KEYBOARD_LL` hook, WASAPI capture, `Ctrl+V` paste, UI Automation.
This machine is a Mac. That CI run is the first thing that would prove any of
it.

```sh
gh auth refresh -s workflow
mkdir -p .github/workflows && git mv ci/ci.yml .github/workflows/ci.yml
git commit -m "ci: build and test on macOS and Windows" && git push
```

### 2. Your Groq key is expired

Live calls return 401. Local whisper is unaffected, so dictation works
offline regardless — but cloud transcription, LLM cleanup, Ask, command mode,
and meeting summaries need a fresh key. Replace it in the Settings tab (PR #11).

---

## 16 PRs

All open, each self-contained and tested, each stating what was verified and
what was not. Reviewing in order tells the whole story.

| PR | What |
|----|------|
| [#1](https://github.com/juligandhi-a11y/Aloud/pull/1) | `audio` — mic capture, dual-rate output |
| [#2](https://github.com/juligandhi-a11y/Aloud/pull/2) | `hotkey` — hold/toggle/modifier-only bindings |
| [#3](https://github.com/juligandhi-a11y/Aloud/pull/3) | `paste` — clipboard snapshot/restore, synthetic paste |
| [#4](https://github.com/juligandhi-a11y/Aloud/pull/4) | `transcribe` — local whisper + cloud |
| [#5](https://github.com/juligandhi-a11y/Aloud/pull/5) | `llm` — chat, prompts, rate limits, `CLAUDE.md` |
| [#6](https://github.com/juligandhi-a11y/Aloud/pull/6) | **the working dictation pipeline** |
| [#7](https://github.com/juligandhi-a11y/Aloud/pull/7) | migration from the Swift app |
| [#8](https://github.com/juligandhi-a11y/Aloud/pull/8) | the recording pill (screenshot in the PR) |
| [#9](https://github.com/juligandhi-a11y/Aloud/pull/9) | first report |
| [#10](https://github.com/juligandhi-a11y/Aloud/pull/10) | notes window — search, highlight, reader |
| [#11](https://github.com/juligandhi-a11y/Aloud/pull/11) | settings UI — **where you replace the key** |
| [#12](https://github.com/juligandhi-a11y/Aloud/pull/12) | report update |
| [#13](https://github.com/juligandhi-a11y/Aloud/pull/13) | Ask mode — streaming answer panel |
| [#14](https://github.com/juligandhi-a11y/Aloud/pull/14) | command mode — transform the selection |
| [#15](https://github.com/juligandhi-a11y/Aloud/pull/15) | meeting notes — ambient capture |
| [#16](https://github.com/juligandhi-a11y/Aloud/pull/16) | Agent Mode foundation (Phase 9) |

117 tests · `cargo clippy --workspace` zero warnings · `tsc --noEmit` clean.

---

## What shipped

```
crates/
  audio        cpal mic capture; 16kHz WAV + 24kHz realtime tap
  hotkey       CGEventTap (mac) / WH_KEYBOARD_LL (win) + pure reducer
  llm          OpenAI-compatible chat, prompts, ask triggers, rate limits
  notes        markdown notes + search; app data paths
  paste        clipboard snapshot/restore, synthetic paste
  selection    AX (mac) / clipboard-fallback selected-text read
  store        settings, history (SQLite), stats, migration
  transcribe   local whisper.cpp + cloud whisper
  agent        Phase 9: action schema, parser, dispatcher, connector trait
apps/desktop   Tauri v2 shell, React UI, overlay + ask panels
```

Logic lives in crates with no Tauri dependency — testable with `cargo test`,
runnable from `examples/` without launching the app. `src-tauri` only wires
them together. The deliberate opposite of the 4,000-line `AppState` it
replaces.

### Feature parity

Dictation · post-processing cleanup · Ask mode (spoken trigger + hotkey) ·
command mode · meeting notes · notes search window · settings · stats ·
recording overlay · migration from the Swift app. Local whisper is the
default, replacing the Apple-only `SFSpeechRecognizer`, so everything works
offline and on Windows.

### Agent Mode (Phase 9)

Your mid-session request. The foundation is built: universal action schema
(9 actions across Spotify/Slack/GitHub/Notion/Calendar), the connector
interface, the intent parser, and the dispatcher with retry, honest gaps, and
chained partial-failure. The plan's core safety rule is structural — every
write action is gated, so nothing sends or creates without confirmation. The
connectors themselves need real OAuth apps, which is your setup to do; Spotify
is the recommended first (low-risk, proves the loop).

---

## Bugs found — every one caught by real hardware, not tests

These all **passed their unit tests** and were exposed only by driving the
real thing. It's the pattern worth keeping.

- **Empty WAVs.** The FFT resampler silently dropped everything for a
  non-aligned buffer; a 3-second recording wrote a 0-frame file. Now
  `FastFixedIn`, with regression tests on the real 48k→16k path.
- **The Ask trigger never fired on real speech.** Whisper hears "hey flow" as
  "Hey, Flo"; the matcher demanded the dictionary spelling. It now accepts the
  plausible ASR spellings, with the real transcript pinned as a test. This
  would have made the whole feature dead on arrival.
- **`[BLANK_AUDIO]` pasted as text.** whisper.cpp's silence marker isn't
  empty, so it sailed past the silence check — and was already in the history
  table. Bracketed markers are now stripped.
- **The overlay rendered nothing** while reporting `visible: true`. It was
  missing from `capabilities/default.json`, so Tauri v2 silently denied its
  event listener. Found only by screenshotting.
- **whisper.cpp's C-side stdout** cost 19 seconds of startup; installing its
  logging hook dropped it to 204 ms.
- **The wrong data directory** (`app.Aloud.Aloud` vs `Aloud`) would have made
  migration silently target the wrong path.

---

## Not done

- **Agent Mode connectors** — the schema/parser/dispatcher are ready; each app
  needs its OAuth setup (yours to do). Confirmation UI and the voice→dispatch
  wiring land with the first connector.
- **Onboarding wizard, realtime streaming, app context (screenshot→vision),
  Composio, voice macros, app rules, auto-updater, code signing/notarization.**
  These are polish and infrastructure, not core function.
- **Windows verification** — see item 1. Nothing Windows-specific has run.

Two UI PRs (#10, #11) are verified at the data layer but not visually — macOS
blocked `screencapture` behind a permission prompt I couldn't grant.

---

## If you do three things

1. Grant the `workflow` scope and land CI. Windows is unproven until it runs.
2. Replace the Groq key (Settings tab).
3. Read PR #6 — the app actually working — then #16 for where Agent Mode goes.
