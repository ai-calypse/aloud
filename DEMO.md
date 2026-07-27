# Demo script — voice-OS meeting

Ten commands, ordered so each one earns the next. Every latency below was
measured on this machine today, not estimated.

**Before you start:** run `pnpm dev` from the repo root (the `.env` key loading
does not survive bundling — a `.app` from Finder has no keys), and run the
warm-up in [§0](#0-warm-up-do-this-before-they-arrive).

---

## 0 · Warm-up (do this before they arrive)

Two things are cold-start traps, both one-time:

1. **macOS Automation prompts.** The first `osascript` call to each app raises a
   permission dialog. Measured: browser tab read was **4.6s cold, 333ms warm**.
   Say commands 4, 6 and 7 once so every app is already granted.
2. **App cold launch.** Reading System Settings took **19s** when the app was
   still launching, **635ms** once open. Always let an app finish opening before
   reading it.

Have Downloads populated (73 files here: 47 png, 10 dmg, 6 zip, 3 pdf) and
System Settings closed.

---

## The ten

### 1. "Open Safari, Notes, and Reminders"
**Multi-app orchestration, one call.** Three apps launch together.

The point to make: this is *one* tool call, not three. A per-app tool would hit
the loop's per-tool budget and stall — verified: 15 apps in a single call, all
15 launched, 11.2s. Say the 15 number; it lands harder than 3.

---

### 2. "Organise my downloads"
**The headline.** 73 files sorted into Images / Installers / Archives /
Documents in one call, with a preview in the confirmation before anything moves.

What to say while it runs: *the model chooses the categories, Rust performs the
moves.* A per-file tool cannot do this — 73 files against a 3-calls-per-tool
budget sorts three files and stops. This is the difference between a demo and a
product.

### 3. "Undo that"
**Everything reverses.** Every file goes back where it was; the folders it
created are swept up.

Do this immediately after 2. A voice agent that can undo a 73-file move is a
stronger trust signal than the move itself — and it means you can re-run the
demo.

---

### 4. "Put my window on the left half"
**Instant, native.** ~800ms.

Worth noting: it reports the geometry it actually achieved rather than trusting
the call. Brave silently ignores height changes — a tool that assumes success
claims a tidy screen that isn't one.

---

### 5. "What page am I looking at?"
**Reads across app boundaries.** Returns the real URL and title of the frontmost
tab. Works with Arc, Chrome, Brave and Safari. ~330ms warm.

---

### 6. "Find all my screenshots"
**Structured search**, then the result grid renders in the panel. Fast because
it is Spotlight, not the screen.

### 7. "Copy them to a folder called Screenshots in my Documents"
**Batch file operation** — chains off 6's results in one call.

⚠️ **Known limit, and say it before they find it:** search returns at most **60
paths**, so a 354-image request collects 60. The result carries a `truncated`
flag so the agent says so honestly rather than claiming all 354. The fix is a
search-scoped collect where paths never round-trip through the model.

---

### 8. "Open System Settings" → then "turn on dark mode"
**The computer-use moment.** Two commands, deliberately.

This is where the AX-tree argument gets made. The agent reads System Settings'
*actual* controls — `Wi‑Fi`, `General`, `Appearance` — with real coordinates,
then clicks the real one. No screenshot, no pixel guessing, no per-app code.

**The numbers to quote** (measured today):

| | AX tree | Vision (screenshot + model) |
|---|---|---|
| Read the screen | **~400ms** | 1-3s to capture, encode, and infer |
| Per click | **~170ms** | plus a full round-trip to re-locate |
| Cost per turn | **~2.3KB of text** | a full image every turn |
| Accuracy | exact element rects | pixel estimates that drift with layout |

The compression number is the one that wins the argument: a raw Finder
traversal is **269 elements / ~26KB / ~8k tokens**, and **235 of those are menu
items inside closed menus** — real elements that do nothing when clicked. We
filter to labelled, on-screen, actionable elements: **27 elements, 2.3KB, an
11.5x reduction.** That filtering is why a traversal per turn is affordable at
all. Anyone who has tried this has hit the token wall; this is the answer to it.

---

### 9. "Click Wi-Fi" (with System Settings open)
**Precision targeting.** Verified today: the click lands on Wi‑Fi and the panel
navigates to Network.

Say what makes it exact — the agent clicks the element's **centre**, computed
from its real rect. The origin of a wide control is its left edge; a table row's
origin is padding. Centre is what a person clicks.

---

### 10. "Email Charlie that we're meeting at 2pm tomorrow, and put it on my calendar"
**Cross-app chain, cloud + local.** Gmail and Calendar via Composio, one spoken
sentence, with a confirmation showing the full recipient address before it
sends.

⚠️ **Optional finale — only run it if you tested it tonight.** This is the one
path I could not verify end to end; it needs your Composio connectors live. If
you have not rehearsed it, stop at 9 and describe this instead of running it.

---

## The architecture slide, in three sentences

> **Layer 1** is 18 structured tools — fast, deterministic, individually
> confirmable. **Layer 2** reads any app's accessibility tree and drives its real
> controls, so there is no per-app code and no workflow to pre-define. Layer 1
> handles what is common; Layer 2 handles everything else, and the agent picks
> between them.

The insight worth stating out loud: **a bigger tool catalogue never closes the
gap.** "Pictures" vs "photos" vs "images" is a description-matching problem in
any fixed-tool design. That is why the second layer exists rather than a
ninetieth tool.

---

## If something fails

- **A tool fails live** — say the honest thing: "that's a tool that needs
  another Automation grant." Then move on. It fails readably, not as a crash.
- **The panel shows the previous run's steps** — that was fixed today (each run
  takes a ticket; superseded runs cannot write to the panel). If it recurs, you
  are on an old binary: restart `pnpm dev`.
- **A confirmation appears for something you did not ask** — same fix, same
  cause. Restart.
- **Escape cancels anything** mid-run, between tools.

## What not to promise

- **Windows** is unexercised — every constant here is macOS.
- **Electron and browser content** are the known AX blind spots. Slack and
  VS Code will traverse poorly; do not demo them.
- **The confirmation gate for computer-use is once per run**, not per click.
  That is deliberate and interim — a hundred-click run cannot ask a hundred
  times — but the target-aware gate (re-ask on a Send button, a payment field)
  is genuinely unsolved. If he asks about safety, this is the honest answer and
  it is a better one than "we prompt the model not to."
