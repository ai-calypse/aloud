---
name: Aloud notch
description: The notch panel's design system — one black surface welded to the camera cutout, everything Aloud shows in a glance.
colors:
  notch-fill: "#000000"
  ink: "#f2f2f2"
  ink-2: "#a1a1a1"
  ink-3: "#8a8a8a"
  hairline: "#1c1c1c"
  well: "#121212"
  edge: "#222222"
  track: "#222222"
  raised: "#262626"
  hover: "#0d0d0d"
  primary: "#f2f2f2"
  primary-ink: "#0a0a0a"
  focus: "#0a84ff"
  ok: "#30d158"
  warn: "#ff9f0a"
  danger: "#ff453a"
  app-action: "#0a84ff"
typography:
  lead:
    fontFamily: "Geist Variable, -apple-system, system-ui, sans-serif"
    fontSize: "30px"
    fontWeight: 600
    lineHeight: 1.1
    letterSpacing: "-0.035em"
    fontFeature: "tnum"
  body:
    fontFamily: "Geist Variable, -apple-system, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.4
  title:
    fontFamily: "Geist Variable, -apple-system, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 500
  meta:
    fontFamily: "Geist Variable, -apple-system, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
  label:
    fontFamily: "Geist Variable, -apple-system, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 500
  number:
    fontFamily: "Geist Mono Variable, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
    fontFeature: "tnum"
  count:
    fontFamily: "Geist Mono Variable, ui-monospace, monospace"
    fontSize: "10.5px"
    fontWeight: 600
rounded:
  key: "4px"
  chip: "6px"
  control: "7px"
  row: "8px"
  track: "9px"
  panel: "24px"
spacing:
  gutter: "14px"
  body: "16px"
  row-y: "6px"
  row-x: "8px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-ink}"
    rounded: "{rounded.control}"
    height: "26px"
    padding: "0 10px"
  button-quiet:
    textColor: "{colors.ink-2}"
    rounded: "{rounded.control}"
    height: "26px"
    padding: "0 10px"
  button-quiet-hover:
    backgroundColor: "{colors.well}"
    textColor: "{colors.ink}"
  button-danger:
    textColor: "{colors.danger}"
    rounded: "{rounded.control}"
    height: "26px"
  chip:
    backgroundColor: "{colors.well}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.chip}"
    height: "24px"
    padding: "0 8px"
  count:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.primary-ink}"
    typography: "{typography.count}"
    rounded: "{rounded.key}"
    height: "16px"
  segmented-track:
    backgroundColor: "{colors.well}"
    rounded: "{rounded.track}"
    padding: "3px"
  segmented-thumb:
    backgroundColor: "{colors.raised}"
    rounded: "{rounded.chip}"
    height: "26px"
  row:
    textColor: "{colors.ink}"
    rounded: "{rounded.row}"
    padding: "6px 8px"
    height: "38px"
  row-hover:
    backgroundColor: "{colors.hover}"
  switch-on:
    backgroundColor: "{colors.primary}"
    width: "34px"
    height: "20px"
  switch-off:
    backgroundColor: "{colors.raised}"
  kbd:
    backgroundColor: "{colors.well}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.key}"
    height: "18px"
---

# Design System: Aloud notch

## 1. Overview

**Creative North Star: "The Cutout That Answers"**

The notch panel is one sheet of pure black that grows out of the MacBook's camera cutout and settles back into it. It is the only surface in Aloud that floats over the user's work, so it behaves like hardware rather than a window: the same black as the cutout (so the seam never shows), concave shoulders welded to the menu bar, and motion that comes from the notch and returns to it. PRODUCT.md's first principle governs every decision here — *"A glance, not a visit."* The user is in an editor, a terminal or a browser; the notch answers its question in under a second and gets out of the way.

The style is the one chosen in `docs/design/notch-styles.html` as "Mix": Vercel's pure black, 1px hairlines and Geist; Linear's icon tabs and the counts on tabs that need you; shadcn's Switch, Slider and bordered segmented control; Apple's notch weld and springs. Density is Control Center, not a dashboard: one fact leads each view, rows sit on hairlines, and nothing is nested in a card. It rejects PRODUCT.md's anti-references by name — **gimmicky glass** (heavy blur and glow, neon accents), **a crowded icon dock** (unlabelled icons to memorise), and **web-dashboard habits** (hero metrics with gradient accents, cards nested in cards, admin-panel chrome).

Motion is part of the system, not decoration. Springs, never durations, for anything physical; one damping ratio for the whole app (only the response varies); close a fifth slower than open; things emerge from the notch, never from their own centre; a size change is the animation. Values live in `src/styles/motion.css`; motion whose target can move mid-flight (a tab underline, the panel's size, the pill's width) runs on the live spring in `src/lib/spring.ts`. Reduced motion is a hard switch that collapses travel and keeps crossfades. The `aloud-motion` skill is the long form.

**Key Characteristics:**
- Pure black (#000) welded to the cutout; depth from tone and hairlines, never shadow or blur.
- Geist for words, Geist Mono for numbers that tick; one lead per view.
- Every destination is named: icon plus label, never an icon alone.
- Fixed width (640px body), height to fit up to a ceiling, every size change on the panel's live spring.
- Only built features render: the registry hides anything not ready.

## 2. Colors

A restrained, monochrome palette on pure black: greys do the work, white is the one inverted action per view, and the system's accent appears only as the keyboard focus ring and inside approval cards drawn as other apps' windows.

### Primary
- **Notch Ink** (#f2f2f2): titles, selected tabs, values, and — inverted — the background of the single primary button in a view (`bg-notch-primary`, ink #0a0a0a).

### Neutral
- **Notch Fill** (#000000): the panel, the pill, the alert. Pure, so the seam with the hardware cutout never shows.
- **Ink 2** (#a1a1a1): subtitles, idle tabs, quiet buttons. 8:1 on black.
- **Ink 3** (#8a8a8a): labels, the status line, footer hints. 6:1 on black — the floor for the smallest text.
- **Hairline** (#1c1c1c): the 1px lines between rows and regions.
- **Well** (#121212): the ground of a control — segmented track, chip, keycap, selected palette row.
- **Edge** (#222222): the 1px border a control draws; also the **Track** of bars and sliders.
- **Raised** (#262626): the selected segment's thumb, a switch at rest.
- **Hover** (#0d0d0d): a row under the pointer.

### Semantic
- **Focus** (system blue #0a84ff): keyboard focus, as an outline outside the control. Native macOS focus is the accent ring; anything else reads as selection.
- **Ok / Warn / Danger** (#30d158 / #ff9f0a / #ff453a): the word on a status chip, an urgent island's dot, a meter near its cap. Always beside a word or a number.
- **App Action** (#0a84ff): only inside approval cards and the alert chip, which are drawn as the acting app's own window and use that app's blue.

### Named Rules
**The Token Rule.** Every colour in a component comes from `src/styles/tokens.css` through a `notch-*` utility (`text-notch-ink-2`, `bg-notch-well`). A hex value or `bg-[var(--…)]` in a component means a token is missing; add it to `tokens.css`, expose it in `index.css`'s `@theme inline`, and register it in `lib/utils.ts`'s tailwind-merge groups, or `cn()` silently drops one of two `text-*` classes.

**The Word-First Rule.** Status is never colour alone (PRODUCT.md, Accessibility). A green "Done" is the word Done, coloured; a red dot is accompanied by "Needs you".

## 3. Typography

**Body Font:** Geist (Geist Variable, bundled; falls back to -apple-system)
**Number Font:** Geist Mono (Geist Mono Variable, bundled)

**Character:** One family in a few weights carries the whole panel; Geist Mono is reserved for numbers that change in place (timers, percentages, counts), so they line up and never jitter. Both are bundled from `@fontsource-variable` and imported by `overlay.tsx`: the CSP is `default-src 'self'`, so no font host loads.

### Hierarchy
- **Lead** (600, 30px, 1.1, −0.035em, tabular): the one fact a view leads with — a time, a percentage. One per view; two leads is no lead.
- **Title** (500, 13px): row titles, tab labels, button labels.
- **Body** (400, 13px, 1.4): sentences — empty states, explanations. Max ~56ch.
- **Meta** (400, 12px): subtitles, chip words, the status line, the closed pill's values (600 there).
- **Label** (500, 11.5px, sentence case): what a group of rows is, and footer hints.
- **Count** (Geist Mono 600, 10.5px): the number on a section tab.

### Named Rules
**The No-Eyebrow Rule.** Labels are sentence case at 11.5px in Ink 3. No tracked uppercase kickers above groups.

**The Approval-Card Exception.** Approval cards keep the system face (`.notch-shell .preview`): they are drawn as Mail's or Slack's own window, and that app would not use Geist.

## 4. Elevation

Flat. The panel is opaque black and casts nothing — a shadow on #000 over the desktop reads as a grey halo and breaks the weld to the cutout. Depth comes from tone (Well under Raised) and from 1px hairlines. The only layering is in time: the palette replaces the body in its flow rather than floating over it.

### Named Rules
**The No-Halo Rule.** No `box-shadow`, no `backdrop-filter`, no glow on anything inside the notch. If it needs to stand out, it is the one inverted button.

## 5. Components

Primitives live in `src/components/notch/` and extend `src/components/ui/` where it already had the shape: `Button` and `Badge` gained `notch*` variants, the ui `Toggle` gained a `notch` tone (exported as `Switch`), and `SlidingPill` learned a horizontal axis. Every interactive primitive has default, hover, focus-visible (the Focus outline), active and disabled states, and its motion goes through motion.css or lib/spring.ts so reduced motion reaches it. The tune page (`/tune.html` → Notch primitives) lays every one out.

### Buttons
- **Shape:** 26px tall, 7px radius, 10px sides (`size="notch"`; `notch-icon` is a 26px square).
- **Primary** (`variant="notch"`): white on black, semibold. One per view — the thing the view is for (Allow, Review, Show plan usage).
- **Quiet** (`notch-quiet`): Ink 2 text; Well fill on hover, Raised on press.
- **Danger** (`notch-danger`): red text; a 15% red fill only on intent.
- **States:** press scales to 0.97; disabled is 50% and inert; focus is the blue outline 2px outside.

### Chips
- **Style:** 24px, 6px radius, Well ground, Edge border, Ink 2 word (`Badge variant="notch"`). Tones `notch-ok`, `notch-warn`, `notch-danger` colour the word only.
- **Count** (`notch-count`): 16px, inverted, Geist Mono — Linear's count, on tabs where something needs you.

### Navigation
- **SectionTabs:** icon + label + optional count, 40px tall, Ink 2 → Ink on hover and when selected. One white 2px underline rides the strip's bottom hairline on a live spring (inset 5px), so a click mid-travel bends it. `role="tablist"`, one Tab stop, ← → / Home / End move and select, ↓ hands focus to the body.
- **SubTabs:** a bordered track (Well, Edge, 9px radius, 3px padding) with a Raised thumb (6px radius) sliding on a live spring — position and width, since labels differ. A lone subsection shows its name instead.
- **Segmented:** the same track at 22px as a `radiogroup`, for a choice inside a view (30 min · 2 hrs · Always). Disabled segments are skipped by the arrows.

### Lists
- **List / Row:** rows on hairlines, no card around them and none per row. 38px minimum, 6/8px padding, an 8px-radius Hover fill. A lead slot (26px icon tile), a title over an optional subtitle, a trailing `Value` (Geist Mono, tabular) or action. With `onClick` the whole row is one button — then it holds no other control.
- **Lead, Label, Empty:** the view's one lead fact; a group's sentence-case label; an empty state that teaches what fills it and offers the one thing to do.

### Controls and meters
- **Switch:** 34×20, Raised at rest, white when on with a black knob that overshoots slightly (`--spring-pop`).
- **Slider:** a styled native range (`.notch-slider`): 4px Track, Ink fill, a white thumb ringed in black that grows on hover and press. Keyboard and accessibility come from the platform.
- **Kbd:** 18px Well keycap in Geist Mono — ⌘K, ⇥, esc.
- **AppIcon:** a `data:` image (Rust's `aloud_selection::app_icon` for Mac apps, `aloud_agent::icons` for services — the only images the CSP loads), a named glyph from `GLYPHS`, or a neutral window. 18 / 26 / 30px.
- **Bar / Ring:** meters with `role="meter"`. The bar's fill is clipped (`clip-path`) so its end stays round and it moves on the compositor; the ring is an SVG stroke. Tones beside a number, never instead of it.

### The section shell (signature)
`features/notch/NotchShell.tsx` is the expanded panel: SectionTabs (+ ⌘K and close), SubTabs with a status line, the view, and a footer naming the keys. Fixed 640px body width so the tab strip never moves sideways; height hugs content up to a 420px body ceiling, and every height change rides the panel's live spring (`useNotchPanel`), so switching tabs mid-morph bends the morph. Views swap with `Materialize`.

Keys: **⌘1–9** go to sections by their *registry position* (⌘2 is Agents in every build, even while Now is hidden in front of it); **Tab / Shift-Tab** step subsections from the chrome and stay native inside the body (so an approval card's buttons stay reachable); **⌘K** opens the palette; **esc** closes the palette, else the panel. The last place the user chose is remembered across opens and launches; a voice run or an approval takes the panel to itself without moving that memory.

**The registry** (`features/notch/registry.ts`) is the whole map from `docs/design/notch-sections.html`: nine sections in ⌘ order, each subsection with an id, name, the features it holds, a `ready` flag and a view. `readySections` hides every subsection that is not ready and every section left empty — there is no "coming soon".

**Adding a feature:**
1. Write the view in `features/notch/views/`, reading live data with `useNotch()` (add the field to `NotchData` in `context.tsx` and supply it from `overlay.tsx` if it is new).
2. In `registry.ts`, set the subsection's `view` and flip `ready: true`. Give the section a `status` (and a `count` if something can need the user). Append new sections; never reorder — the position is the shortcut.
3. If it can be done from a keystroke, add an action to `actions.ts`; ⌘K finds it, and an island can name it.
4. If it is live while the notch is closed, push an island from its crate (below).
5. Add a fixture to `tune.tsx` and a test for any logic that can silently be wrong.

### Islands (signature)
A live activity any crate can show. `crates/islands` owns the logic and the type:

```rust
Island { id, kind: Live | Alert, priority: Low | Normal | High | Urgent,
         icon: Option<String>, title, detail: Option<String>,
         actions: Vec<IslandAction { label, action, args }>, expires: Option<u32 /* ms */> }
```

- **Push** with `crate::islands::push(app, island)`; **remove** with `crate::islands::remove(app, id)`. Same id updates in place and keeps its place among equals; an identical re-push sends nothing but renews `expires`. Ranking: priority, then arrival.
- **Kinds:** `Live` draws beside the camera in the closed notch (`IslandPill`: ranked, alternating sides, never under the cutout, a count past four) and as a row in Now › Overview. `Alert` is the chip that drops from the notch for four seconds (paused while pointed at) — the write toast is one; it goes through `overlay::show_toast` so it never takes the window from a panel the user is in.
- **icon:** a `data:` URI, or a glyph name the webview knows (`agents`, `download`, `timer`…; add to `GLYPHS`). **detail** is short enough for the pill ("2 running", "18:42").
- **Wire:** one `island` event per change, `{ change: { op: add | update | remove, value }, order }` — the webview applies it with `applyIsland` and orders by Rust's ranking; `islands()` is the snapshot for a late mount; `dismiss_island(id)` is the user being done with one.

### The action registry and ⌘K (signature)
`features/notch/actions.ts` is one list of things the notch can do: `{ id, title, keywords, section, subsection, args, run(ctx, args) }`. Island buttons name an id (Rust sends names, never code), ⌘K lists every action that needs no arguments, and the voice agent can call ids next. `run` resolves true when it did what it says, which is when an island that offered it may go. `runAction` fails — never throws — for an unknown id, missing arguments or a throwing run.

The palette (`Palette.tsx`) opens in the body, in its flow, so the panel grows to fit on its own spring; the view stays mounted underneath. It searches ready sections, subsections, their features and runnable actions — built from the same registries, so it cannot offer what is not there. Approval cards ignore keys from inside the palette (`fromPalette`): its Enter must never allow the write under it.

## 6. Do's and Don'ts

### Do:
- **Do** weld every notch surface to the cutout in pure #000, and grow it out of the notch on `useNotchPanel`'s spring.
- **Do** name every destination: icon plus label in SectionTabs, a word on every status chip ("Named, not memorised").
- **Do** lead each view with one fact in the 30px Lead, and put everything else on hairline rows.
- **Do** give each view exactly one `variant="notch"` button — the thing it is for — and make the rest quiet.
- **Do** use `notch-*` utilities for every colour, radius and size, and register new tokens with tailwind-merge.
- **Do** write an empty state that says what fills the view and offers the one thing to do.
- **Do** keep numbers that change in Geist Mono with tabular figures.
- **Do** push live state through the island queue from the crate that owns it; never draw a second strip beside the notch.

### Don't:
- **Don't** use **gimmicky glass**: no blur, glow, neon accents or glass as decoration (PRODUCT.md).
- **Don't** build **a crowded icon dock**: no unlabelled icon rows to memorise.
- **Don't** bring **web-dashboard habits**: no hero metrics with gradient accents, no cards nested in cards, no admin-panel chrome.
- **Don't** put a shadow, a halo or a backdrop filter on anything in the notch.
- **Don't** add a "coming soon" view or a disabled tab: leave `ready: false` and let the registry hide it.
- **Don't** reorder `SECTIONS`: a section's position is its ⌘ number.
- **Don't** resize the overlay window to animate; morph the shape inside it.
- **Don't** write a second spring or invent a duration in a component: CSS motion from motion.css, retargetable motion from lib/spring.ts.
- **Don't** colour a status without its word, or put content under the camera cutout — it has no pixels.
- **Don't** open the palette, or anything else, as a page-level modal over the user's work.
