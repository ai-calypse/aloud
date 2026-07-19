# Morning Report — 19 July 2026

Overnight: the frontend was reorganised into a design system, and the motion
you asked for is in. Four PRs (#40–#43), all merged to `main`.

The UI looks the same where it should and different only where you asked.
Every claim below was checked by driving the interface, not by reading the
diff — the exceptions are listed at the bottom.

---

## What you asked for, and where it is

### Animations

**Sidebar pill** — one chip behind the rows, sliding on `transform` with
`cubic-bezier(.32,.72,0,1)` over 400ms. JS sets a target value; the compositor
runs every frame, so it does not stutter while React renders the pane arriving
beside it.

One deliberate change from your spec, and it matters: you described
`translateY(row × 36px)`. The sidebar has a **divider between the app and
settings groups**, so the rows are not evenly pitched. Connectors sits at
`303px`; the constant would have placed it at `252px`, drifting by the
divider's height for every row below it. It measures the active row instead.

**Pane materialize** — content enters blurred and displaced, then focuses and
settles, as you described:

```css
from { opacity:.4; filter:blur(4px); transform:translateY(var(--materialize-from,7px)) }
to   { opacity:1;  filter:blur(0);   transform:none }
```

`--materialize-from` is signed by the direction you moved through the list, so
the pane agrees with where your eye just travelled. Restarted with the
`animation:none` → force reflow → reassign trick.

**The listening bars.** You were right that they looked unpolished, and the
reason is worth knowing: they were **not listening**. Each bar ran a canned
0.9s sine loop, and the measured level only squashed the whole row — so the
motion was identical whether you spoke or sat in silence.

They are now a real scrolling meter: fourteen thin bars, each one a 50ms
sample of what the mic actually heard, newest on the right, with fast attack
and slow release so peaks fall away over ~300ms instead of strobing between
syllables. You can see the shape of a word in it.

**The pill entrance** — it is a glass surface, so it now materialises rather
than sliding in: blurred and slightly small, coming into focus at size with a
touch of spring overshoot. The bars stagger up behind it.

**Reduced motion** is one global switch. It collapses durations to `1ms`
rather than setting `animation: none`, because killing an animation outright
can strand an element on its opening frame — invisible, offset, or mid-blur.
Verified: content still settles at full opacity and zero blur.

### Architecture

`src/styles/` is the single source of truth now: `tokens.css` (every colour,
radius, shadow and type size), `base.css`, `motion.css` (durations and curves),
plus one file per window. Components use semantic names — `bg-surface`,
`text-ink-2`, `rounded-card` — instead of the 40 repeated arbitrary values they
had.

Shared primitives replaced the copy-paste: `SearchField`, `PanelShell`,
`Surface`, `Caption`, `GridRow`, and the app's button styles folded into the
one `Button` as variants. Loose CSS went from 596 lines across six files to
only what genuinely has to be CSS.

The architecture is written up in `CLAUDE.md` under **Frontend architecture**,
so the next change lands in the right place.

---

## Four bugs this turned up

All four were live in `main` before last night.

**1. `var(--warning)` was undefined in the agent panel.** Only the main window
imported the palette, but `agent.tsx` and `lib/tool-views.tsx` both use it.
Those warnings rendered in whatever colour they happened to inherit.

**2. Onboarding was invisible in light mode.** Its stylesheet assumed a
permanently dark backdrop — near-white translucent fills — while rendering
inside a window that follows the OS appearance.

**3. Every Edit/Delete/Add link silently lost its colour.** `tailwind-merge`
groups classes by parsing the name, and our custom `text-ui` (13px) and
`text-brand` (accent) both look like `text-<unknown>` to it, so it filed them
in one group and kept only the last. This typechecked, linted and built clean —
I caught it by reading computed styles off the rendered buttons. `cn()` now
declares the custom groups, and that list has to stay in step with the
`@theme inline` block in `index.css`.

**4. The sliding pill never positioned on first paint.** Its effect keyed on
the active id, but the target element arrives from a callback ref one render
later.

---

## Needs you

**GitHub Actions is blocked on billing.** Every job fails within seconds:
*"recent account payments have failed or your spending limit needs to be
increased."* Nothing has run server-side for any PR this week. Local checks
(`cargo test --workspace`, `clippy`, `tsc`, `oxlint`, `vitest`, `vite build`)
are all clean, but that is not the same thing.

The consequence worth naming: **the Windows path has never been executed.**
`WH_KEYBOARD_LL`, WASAPI capture and `Ctrl+V` paste are written and unproven,
and the CI matrix is what would prove them. `ci/ci.yml` is also still outside
`.github/workflows/`; moving it needs a token with the `workflow` scope:

```sh
gh auth refresh -s workflow
mkdir -p .github/workflows && git mv ci/ci.yml .github/workflows/ci.yml
```

**No Composio key on this machine.** The connector caching, the
stale-while-revalidate push and the disconnect flow are unit-tested and driven
against stubs, never against the live API.

---

## What I could not verify

- **The listening meter with a real microphone.** I drove it with synthesised
  level values in a browser. The audio path is unchanged, but I have not held
  the hotkey and spoken into it.
- **Anything in the real app window.** The dev instance had no window open to
  drive, so every visual check ran against components in a headless browser
  with a stubbed Tauri IPC.
- **Windows.** As above — and note #37 touched `installed_apps`, which is a
  no-op stub there.

`PROGRESS.md` has the detail.
