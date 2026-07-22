# Future roadmap — what the agent could become

Ten things Aloud's agent could do once it has tens of connectors and can drive
the Mac itself. Each was prototyped as a working, animated demo built from real
[AI Elements](https://elements.ai-sdk.dev/components/) in a black panel hanging
off a menu bar; this file is what survives that page, written so it can be built
from without rebuilding the prototype.

None of it is committed. It is a menu.

## The constraints every idea here answers to

These are what make the list different from the same exercise on a chat product.
An idea that ignores them is an idea for a different app:

- **Your hands are busy.** You are dictating *into another app*, mid-task. A
  text input is usually the wrong question to ask.
- **The surface is small and glanceable.** It hangs under the notch. Anything
  that only reads well in a wide column does not belong.
- **The stakes are real.** It holds write access to your mail and calendar. A
  wrong action is not an inconvenience, it is a sent email.
- **You are one keypress from talking.** The cheapest possible reply is a spoken
  one, so offering something speakable beats offering a field.

## Build order

If only three get built: **01, 03 and 04.** They pay off *before* the connector
count grows — they make the agent safer and stop it stalling on the commands it
already handles. **09 is half-built**: the panel already shows the queue the
tool loop used to discard.

## Getting the components back

Every element named below is vendored with the shadcn CLI, from `apps/desktop`:

```sh
yes n | pnpm dlx shadcn@latest add https://elements.ai-sdk.dev/api/registry/<name>.json
```

`yes n` matters: the CLI offers to overwrite `ui/button.tsx` and `ui/select.tsx`,
and saying yes drops Aloud's own button variants (`accent`, `plain`,
`plain-danger`) and its motion-token transitions. Always check `git diff` on
`src/components/ui/` after installing.

Already vendored and in use: `tool`, `confirmation`, `queue`, `message`,
`shimmer`. Everything else below needs installing.

One rule learned the hard way: **do not accept `code-block`.** It highlights
with shiki, which took the bundle from 972KB across 7 files to 11MB across 308
(a 608KB wasm blob, a 762KB emacs-lisp grammar, every theme) to colour a flat
JSON object. `tool.tsx` has a local `Json` component in its place — keep that
edit if `tool` is ever re-pulled.

Two more traps: `Shimmer` renders a `<p>` by default (pass `as="span"` inside a
paragraph), and the registry's `context` reads usage fields `ai` 7.0.30 does not
declare (widen locally rather than moving the pin).

---

## 01 · Approve the plan, not every write

**What it is.** When a command turns into several actions, the panel shows the
whole plan and takes one decision — instead of stopping you at each write.

**Why.** Today every write blocks on its own confirmation. With thirty
connectors a single command could hold four of them: four interruptions while
you are mid-sentence in another app. One gate is also *safer*, because you
approve a shape you can see rather than the third prompt you have stopped
reading.

**UI and animation.**

1. `Shimmer` line — "Working out what that takes…" — while the round resolves.
2. The `Plan` card grows in, titled with the shape of the work ("Three actions,
   two of them writes"), holding a `Queue` of the steps with hollow indicators.
3. A single `Confirmation` below it: *Let me pick* / *Run all*.
4. On approval the confirmation leaves and the queue items strike through one
   at a time as each runs — `QueueItemIndicator completed` and
   `QueueItemContent completed` carry both states already.
5. The closing `MessageResponse` fades up under the collapsed plan.

The panel is springing to fit throughout; nothing here needs its own transform.

**Elements.** `plan`, `queue` ✓, `confirmation` ✓

**Touches.** The gate currently lives in `crates/agent/src/tool_loop.rs`, which
awaits a per-call decision. Batching means asking once before the loop starts
executing a round — the `Planned` event already carries exactly that list.

---

## 02 · Point at it and ask

**What it is.** Hold the key, point at something on screen, ask. It reads the
region under your pointer and answers, showing you exactly what it looked at.

**Why.** The fastest possible "what is this error" — no screenshot, no paste, no
switching windows. Showing the crop matters as much as the answer: it is how you
know it read the right thing, and it is the difference between a tool you trust
and one you double-check.

**UI and animation.**

1. A thin representation of the screen region appears in the panel.
2. The crop rectangle **pops** onto it — `scale(0.4) → 1` on `--spring-pop`,
   which is the one place in this list that wants overshoot, because it is a
   thing landing on a target.
3. A monospace label under it names the source: `Xcode › Build log`.
4. `Shimmer` — "Reading what is under your pointer…" — then the answer replaces
   it as a `MessageResponse`.

**Elements.** `message` ✓, `shimmer` ✓, `sources` (for citing the app/region)

**Touches.** `crates/screen` already captures context; this needs the pointer
position and a crop around it, plus the existing ask path. The crop image is the
new artefact — everything else exists.

---

## 03 · Answer it without touching the keyboard

**What it is.** When it needs one detail — which calendar, which account — it
offers the options instead of asking an open question.

**Why.** A text field is the wrong question to ask someone who is dictating.
Options can be spoken, clicked, or ignored, and one of them doubles as "stop
asking me this". This is the single most common reason a voice command dies
halfway.

**UI and animation.** The question as a muted line, then a `Suggestions` row
whose chips arrive staggered (~45ms apart) on the same rise-and-settle the Ask
answer's parts use. Picking one replaces the whole block with the result.

Include a third chip that sets the preference (*"Ask me every time"* → its
inverse), so the interruption can retire itself.

**Elements.** `suggestion`

**Touches.** Needs a way for the tool loop to ask a question mid-run and await
an answer — the same one-shot channel shape `PendingConfirmation` already uses
in `src-tauri/src/agent.rs`, carrying options instead of a bool.

---

## 04 · Take it back

**What it is.** After a write lands, a short window to undo it — pull the mail,
revert the event.

**Why.** Voice misfires in ways typing does not: "send to Sam" heard as "send to
Sara" is one phoneme and an unbounded cost. A confirmation gate catches the
wrong *action*; it does not catch the wrong *argument*, because both look right
until you see the result.

**UI and animation.** The finished `Tool` collapses to its header, and a
`Checkpoint` row appears beneath it with a counting trigger — "Undo — 8s". It
should fade out when the window closes rather than vanishing, so the affordance
expiring is visible. On undo, the tool header flips to `output-denied` and a
one-line confirmation replaces the checkpoint.

**Elements.** `checkpoint`, `tool` ✓

**Touches.** Per-toolkit undo. Gmail has send-cancel windows; Calendar needs the
inverse call recorded at dispatch. Worth a small `Undoable` trait in
`crates/agent` next to the dispatcher rather than special-casing in the panel.

---

## 05 · What you might say next

**What it is.** After an answer, two or three follow-ups you can just say.

**Why.** Keeps you in voice. Without them the second question costs a keypress
and a re-think, which is where most sessions end at one command — and the
follow-up is usually the valuable one.

**UI and animation.** A `Suggestions` row under the answer, arriving a beat
*after* it settles so it does not compete with the reading. Same stagger as 03.

**Elements.** `suggestion`, `message` ✓

**Touches.** One extra field on the ask/agent completion, or a cheap second call.
Cheap enough to try both and measure — `timings.sqlite` already records the
stages.

---

## 06 · Think harder, and show it

**What it is.** Hard multi-step requests escalate to a reasoning model, and you
watch it work rather than watching a spinner.

**Why.** Reasoning is deliberately off — `reasoning_for()` in `crates/llm`
pins `gpt-oss-20b` to `effort: low, include_reasoning: false`, and that is a
large part of why Ask answers in ~600ms. Some questions are worth the trade, but
only if the wait is legible: four seconds of visible reasoning reads as
thorough, four seconds of nothing reads as broken.

**UI and animation.** `ChainOfThought`, open by default, with steps that move
`pending → active → complete` as the run advances. Intermediate findings attach
to their step as `ChainOfThoughtSearchResult` badges — for a calendar search,
the candidate slots. The answer follows once the last step completes.

Each step's status change is the animation; do not add another.

**Elements.** `chain-of-thought`, `reasoning`

**Touches.** A second model in settings, a rule for when to escalate (or an
explicit "think about this" trigger phrase, which is cheaper and more honest),
and passing `include_reasoning: true` through the stream.

---

## 07 · What this run is costing

**What it is.** For a long run, what it has read and how many rounds it has
taken.

**Why.** The tool loop can go five rounds (`MAX_ROUNDS`) and today every one of
them is invisible. With thirty connectors the tool list alone becomes the
expensive part of the prompt, and "why did that take nine seconds" should be
answerable from the panel rather than from the timings database.

**UI and animation.** A `Context` meter in the panel footer, filling as rounds
accumulate, with the round counter in the `Shimmer` line above it ("Round 2 of
5 — reading threads…"). Quiet: this is diagnostic, and it must never outrank the
run itself.

**Elements.** `context`, `shimmer` ✓

**Touches.** The provider's `usage` block is already recorded onto the tracing
span (`crates/llm`); this surfaces the same numbers live instead of after.

---

## 08 · Turn it into a verb

**What it is.** When you have done the same sequence a few times, it offers to
name it. Then you say the name.

**Why.** This is where connectors compound. One command that opens Zoom, mutes
you and pulls up the notes is worth more than three that each do one thing — and
the app is the only thing in a position to notice it is the third Monday.

**UI and animation.** The run finishes as a `Task` ("Did three things") holding
its `TaskItem`s. A `Suggestions` row offers *Save as "standup setup"*. On save
the Task **collapses and retitles itself** to the new name in one move — that
collapse is the animation, and it is the whole idea made visible: three things
becoming one.

**Elements.** `task`, `suggestion`

**Touches.** History already stores runs. Needs sequence matching over recent
runs, storage for named workflows, and a spoken trigger — `crates/store` and the
existing macro machinery are the closest homes.

---

## 09 · Keep talking while it works — *half built*

**What it is.** A second command while the first is still running gets queued
instead of colliding with it.

**Why.** Speaking is faster than executing. A calendar write takes two seconds
and the next sentence is already out of your mouth; today that fights the run in
flight. Queueing turns the mismatch into the feature.

**UI and animation.** The running `Tool` stays at the top; queued utterances
stack below in a `Queue`, each arriving on its own fade-up. As the run finishes,
the top queued item promotes into a Tool and the queue shortens.

**Already done.** `LoopEvent::Planned` surfaces the batch the tool loop had been
discarding, and the panel renders it as "N still to do". What remains is the
*input* half: letting a second dictation enqueue rather than supersede, which is
`Session::owner` in `pipeline.rs` — today a new role takes the session over.

**Elements.** `queue` ✓, `tool` ✓

---

## 10 · Bring what you are looking at

**What it is.** "Send this to Priya" — it works out what *this* is from what is
in front of you, and what it picked up is part of the confirmation.

**Why.** The most natural sentence in voice is the one full of pronouns, and it
is the one a text box cannot serve. The risk is that a wrong guess is invisible,
so the guess has to be the thing you approve — not a detail behind a disclosure
triangle.

**UI and animation.** A `Task` — "Picked up from what you were looking at" —
listing what it resolved (`TaskItemFile` for the frontmost document, the
recipient it inferred and from where). Then the `Confirmation`, whose reject
action is *Pick a different file* rather than a bare Cancel: the likely failure
is a wrong guess, so the escape hatch should fix it rather than abandon.

**Elements.** `task`, `attachments`, `confirmation` ✓

**Touches.** `crates/selection` reads the focused app; this needs the frontmost
document path and enough context to resolve a recipient.

---

## Notes on the prototype

The demos lived at `apps/desktop/ideas.html` (deleted with this commit; see the
history of `src/ideas.tsx` to run them again). Two things that page proved worth
keeping in mind:

- **Draw new ideas on the real surface.** Several of these looked obvious in a
  wide column and had to change shape to fit under a notch. A mock in a card
  will lie to you.
- **`data-surface="floating"` is a behaviour, not just a palette.** It locks
  height, kills scrolling and disables text selection, because overlay windows
  are click-through and drag-only. Any page that opts in for the dark palette
  must opt back out of the rest.
