# Morning Report — 20 July 2026

Six open PRs, stacked. Nothing merged into `main`, as asked.

---

## Merge order

Each PR's base is the one above it, so every diff shows only its own work.
GitHub retargets each to `main` as its parent merges — so merge **top to
bottom** and the stack unwinds itself.

| # | Branch | What |
|---|---|---|
| **#61** | `feat/phase-2-hotkey-recorder` | Phase 2 — recorder, command mode, eight hotkey defects |
| **#69** | `perf/dictation-latency` | Latency — ~320ms measured off every dictation |
| **#70** | `feat/phase-3-prompt-vocab` | Phase 3 — prompt runner, banner, vocabulary, output language |
| **#71** | `feat/phase-4-run-log-v2` | Phase 4 — Run Log pane |
| **#72** | `feat/phase-6-pipeline` | Phase 6 — instruction guard, offline fallback, typed Ask |
| **#73** | `feat/phase-7-overlay-display` | Phase 7 — overlay style, display picker, permissions, build stamp |

**Close #62.** Its Run Log pane is not in the tree; #71 supersedes it.

---

## The three things you asked about

### 1. "Issues with the hotkeys" — eight of them, all reproducible

Tracing the recorder end to end turned up eight defects. Each has a regression
test named for the failure. The two worst:

**Capture wedged the keyboard.** Recording swallows every key while armed,
key-ups included — so a modifier you were holding when you clicked Record
stayed "pressed" in the matcher forever. One phantom modifier fails the
exact-match rule on every modifier-only binding, and **the dictate key was dead
until you relaunched the app.** `Input::Reset` was the designed escape hatch
and was constructed nowhere outside a unit test.

**Escape cancelled and then immediately started a second recording.** `active`
was cleared but the keys were still physically down, so the Escape *key-up*
re-derived the binding. You got your cancel plus a dictation you never asked
for. That fires on the obvious gesture: keep holding, tap Escape.

Also: clearing a binding mid-hold stranded the role forever (mic open until
relaunch); a bare-key binding swallowed every chord containing it, so binding
Space ate Cmd+Space before Spotlight saw it; modifier key-ups were swallowed
unbalanced, leaving apps holding a Command they never saw released; Escape was
recordable but could never fire, and there was no keyboard way out of recording
at all; recording Cmd+Return silently bound "hold Command"; and a ten-second
capture reverted anything you changed on the settings pane meanwhile.

The conflict warning also said "the more specific one wins", which is false —
ordering is not exclusion, both roles fire against one recording.

### 2. "Remove invoke it from command mode" — done, and it took a lot with it

A selection is now the only way dictation rewrites. Nothing selected, you
dictate. Rewriting on demand is the Rewrite hotkey, which already existed.

The manual gesture was not worth its cost. Three of the four offered modifiers
collide with a typical dictate key, so it shipped alongside a routine whose
only possible output was telling you your setting does nothing — which in turn
needed `Config::permitted_extra`, an exception to the matcher's exact-match
rule. **That rule is the reason `crates/hotkey` exists.** Deleting the picker
deleted the exception; the matcher is back to one rule with no special case.

### 3. "A long wait on transcribing" — transcription was not the problem

The stages *around* it were. Measured, not reasoned about:

| | Cost | Fix |
|---|---|---|
| Cleanup paid for reasoning tokens every dictation | **~320ms** | per-model `reasoning_effort`, ported from voice-dict's `ModelConfiguration` |
| Every request built its own connection pool | 72-105ms × 2 | one process-wide `reqwest::Client` |
| The rate-limit breaker was dead code | a wasted request after any 429 | process-wide `CooldownTracker` |
| Clipboard restore sat on the critical path | 150ms | moved off-thread |
| Model load happened inside your first dictation | 300-1500ms once | `warm_up` at launch |

Same prompt, same sample, four runs each:

```
before  542 / 986 / 772 / 844 ms   (mean 786)
after   425 / 407 / 441 / 582 ms   (mean 464)
```

Output quality is unchanged. `crates/llm/examples/cleanup_latency.rs` is how
you re-measure it.

The breaker is visible in that example: a run following a 429 now fails in
**76ms without making a request**. It used to be a full round trip, up to the
30-second timeout, with the pill up the whole time.

**A correctness bug fell out of the same trace: app rules had never fired.**
`frontmost_app` read `AXTitle` off the application element, which is empty for
most apps — every recorded `paste` span logged `target_app: "unknown"`. Your
configured Cursor rule has never once applied.

**What did not pay off, kept or backed out honestly.** `n_threads` is a
measured no-op on macOS (157-215ms at 9 threads vs 154-236ms at 4 — Metal owns
the encoder). Kept only because Windows has no Metal, and the comment says so
rather than implying a win. I also tried disabling whisper's
temperature-fallback loop and **backed it out**: no measurable gain on clean or
deliberately noisy audio, and it trades accuracy on hard audio for nothing.

---

## Roadmap progress

**Complete and in review:** Phases 2, 3, 4, 6, 7 — plus the latency work,
which is not a phase.

**Not done:** 5, 8, 9, 10, 11. Honest reasons:

- **5 (audio persistence)** — now genuinely unblocked. #71 landed the
  `PRAGMA user_version` migration it was waiting on; this is the next one to
  pick up.
- **8 (updates)** — blocked on generating a signing keypair. That is a decision
  about key custody, not code, and not one to make on your behalf overnight.
- **9 (screen context)** — XL, a new crate, and a TCC permission the app has
  never requested.
- **10 (streaming)** — a pipeline rewrite. Smaller than the roadmap thinks (see
  below), but still not an overnight job.
- **11 (surfacing)** — moderate, and the least urgent.

### Seven claims in the docs that were false

A plan built on these plans the wrong work, so they are corrected in
`ROADMAP.md`:

- **The Groq key is not expired.** `PROGRESS.md` said it was; it returns 200,
  and Phase 3's verification ran against it.
- **4.1 did not ship in #62** — that pane is not in the tree.
- **7.2 was substantially already built** — the active-monitor mode and the
  primary fallback both existed.
- **The overlay is top-centre on the cursor's monitor**, not bottom-centre.
- **`crates/audio` already streams** — `Chunker` and the PCM tap are in
  production use by meeting mode, which transcribes chunks concurrently as they
  arrive. Phase 10's real work is a chunk-level entry point in
  `crates/transcribe`, not the audio half.
- **11.2's blocked list is mostly wrong** — 5 of its 7 steps are callable today.
- **`frontmost_app` returned None for everything.**

---

## What I got wrong, and what caught it

Every phase was implemented and then **adversarially reviewed by an agent told
to assume the work was wrong**. That found real bugs in all four. The worst two
would have reached you:

**Phase 3 pasted the literal word `EMPTY` into documents.** With an output
language set, filler speech ("um, uh") made the model return the EMPTY
sentinel, and the sentinel check had been disabled while translating. It was
ported from the wrong Swift function — `sanitizeVerbatimTranslation` belongs to
a minimal literal-translator path carrying no sentinel instruction, which is
the precondition its own comment names. **Two tests and an example were
asserting the broken behaviour**, so the suite locked the bug in.

**Phase 4's "Re-run cleanup" could overwrite a dictation with UI prose.** The
shared command returned an explanatory English sentence when a result was
empty — fine for the Prompts pane that displays it, catastrophic for the Run
Log, which writes the value straight back into your transcript. Permanently, no
undo.

Also caught: Phase 6's offline fallback would have put a **150MB download
inside the dictation you were waiting on** (a cloud user has never had the
local model); and Phase 7's build stamp reported a **real, plausible, wrong
commit** — a build on the Phase 7 commit stamped the Phase 6 hash, which for an
About row is worse than "unknown".

Two process mistakes worth naming: a background agent reverted `matcher.rs`
between my green build and my commit (caught, redone, verified), and my first
commit swept in three scratch benchmark files an agent had left in the tree
(removed by amend).

---

## ⚠️ What is not verified

Stated plainly, because the repo's convention is to say so:

- **Nothing was driven in a real window.** No pane was clicked, no tray item
  used, no overlay seen. Logic was verified at the crate level and against the
  live Groq API; the UI was not.
- **The hotkey recorder had no hardware run this pass.** The matcher fixes are
  covered by unit tests against the real reducer, but the event tap was not
  re-exercised — the machine's screen was locked, which makes every
  Accessibility call return `kAXErrorCannotComplete` (-25204), in Swift as well
  as Rust. That also blocked confirming the `frontmost_app` fix, though its
  parsing is unit-tested including the Electron-helper case.
- **The instruction guard has never fired against a real model.**
- **Windows is entirely unexercised**, as everywhere in this repo.
- **CI still has not run.** GitHub Actions remains blocked on billing.

---

## Where to start

1. Merge the stack top to bottom; close #62.
2. Pick up **Phase 5** — unblocked now, and the migration it needs exists.
3. Decide the **Phase 8 keypair** question; nothing below it can be tested
   until you do.
4. Next time the machine is unlocked, run
   `cargo run -p aloud-selection --example frontmost`. It now prints the AX
   error code and will confirm the app-rules fix in one line.

`PROGRESS.md` has the detail.
