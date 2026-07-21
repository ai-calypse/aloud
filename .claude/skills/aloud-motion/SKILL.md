---
name: aloud-motion
description: >-
  How motion in this app is made to feel the way it does — springs over
  durations, interruptible where the user can redirect it, things emerging from
  where they came from, and the failure modes that render fine and still feel
  wrong. Use when adding, changing, or reviewing any animation, transition or
  gesture response in apps/desktop/src: a new overlay or panel, something that
  opens/closes/expands/collapses, a hover or press state, a value that changes
  while on screen, a drag, or any request phrased as "animate X", "make X
  smooth", "add motion to X", "make it feel native/fluid/like the Dynamic
  Island", or "it feels janky/cheap/snappy-but-wrong".
---

# Motion in Aloud

The feel is not a number. It comes from four decisions, made the same way every
time, and any UI in this app that makes them the same way will feel like the
rest of it.

Values live in `src/styles/motion.css` and `src/lib/spring.ts`. Read them, do
not restate them here — this file is about *which* to reach for and *why*.

## 1. Pick the mechanism first

This is the call that separates motion that feels alive from motion that
stutters, and it is made before any value is chosen.

**Can the target change while the thing is still moving?**

- **No** — the animation runs start to finish unmolested. Use CSS with the
  spring tokens from `motion.css`. The compositor owns every frame and React
  never sees them. This is the default and most motion should be this.
- **Yes** — a status that changes three times in two seconds, a drag, a value
  the user can redirect, anything retargeted mid-flight. Use the live spring in
  `lib/spring.ts`.

The reason it matters: the tokens in `motion.css` are real spring solutions
*sampled into `linear()`* — the shape of a spring, baked flat. Once baked it is
a fixed curve, and a fixed curve redirected halfway **restarts from zero
velocity**. You see it as a stall-then-restart: the thing is travelling, hitches
for a frame, sets off again. `lib/spring.ts` integrates the oscillator live, so
a retarget only changes a term in the acceleration — the motion in progress
carries straight through. That property is the whole reason the file exists.

Cost of getting it wrong in each direction: a live spring where a CSS token
would do burns main-thread frames for nothing. A CSS token where the target
moves produces the hitch, and no amount of curve-tuning removes it.

## 2. The rules that produce the feel

**Springs, not durations.** A duration is a schedule; a spring settles. Reach
for a duration only for things that are not physical — a shimmer, a marquee, a
crossfade of pure opacity.

**One damping ratio across the app; vary only the response.** This is what makes
separate components read as one system. When something needs to feel faster,
make it faster — do not also make it bouncier, or it stops belonging to the same
app. Underdamped (a slight overshoot on arrival) reads as an object landing;
critically damped reads as a value changing. Pick by which one it is.

**Close a little slower than open.** Roughly a fifth. An equal open and close
reads as a box toggling; a close that lingers reads as something with mass
settling back. This asymmetry is doing more work than any single curve.

**Things emerge from where they came from.** A panel that belongs to the notch
grows out of the notch. A popover belongs to its button. Default CSS grows
everything from its own centre, which is almost always the wrong origin and is
the single most common reason a correct-looking animation feels generic.

**A size change *is* the animation.** A container whose contents change width or
height must animate to the new size. Snapping between two sizes is the clearest
tell of web UI in a native app. If the natural size is `auto` or `max-content`,
measure it and drive the real value — see `useMorphWidth` and `useNotchPanel` in
`overlay.tsx` for the two shapes this takes.

**Squash anisotropically.** When content is replaced, compress it more on one
axis than the other. Equal scale on both axes reads as a zoom; unequal reads as
something being squeezed through a slot. Tune the *ratio* between the axes, not
either number alone.

**Blur while moving, sharp at rest.** A little blur during a transition sells it
as one thing changing rather than two things swapping. It is also what hides the
sub-pixel imprecision of a fast morph.

**Frequency governs duration.** The more often a user sees it, the shorter and
subtler it must be. Something that fires three times per dictation gets a beat;
something they trigger deliberately can take its time.

**Everything that enters must exit — and the surface has to outlive the exit.**
An element cut off mid-frame is worse than one that never animated. If the thing
that makes it visible (a window, a route, a conditional render) goes away on the
same tick, the exit never renders. Hold it open, then tear it down.

**Reduced motion is a hard switch, not a slower one.** The OS setting is the
user saying movement makes them unwell. Collapse the travel; never merely
lengthen it. `motion.css` has the global switch, and `lib/spring.ts` reads it
directly because the CSS sweep cannot reach a JS-driven spring.

## 3. Surfaces that change shape

A thing that merely moves or fades needs only the section above. A thing that
*grows, opens, or changes silhouette* has four more decisions, and getting any
of them wrong caps how good the rest can look.

**Never resize the window. Oversize it once and animate inside it.** Give the
surface a fixed transparent canvas larger than anything it will ever reach, and
never touch it again. Resizing an OS window per frame stutters; resizing a shape
inside a still window does not. Make the canvas click-through and transparent
and it costs nothing to be generous. This is the decision that makes everything
else possible — get it wrong and no amount of easing rescues it.

The corollary: with nothing centred in the canvas any more, each surface owns
its own offset from the canvas edge, and the canvas can then grow without
dragging unrelated surfaces around the screen.

**A silhouette is parameters that interpolate, not a shape you swap.** If the
outline itself changes — corners opening, a concave shoulder, anything a
`border-radius` cannot express — build it as a path whose *inputs* are animated
and rebuild the path each frame. Do not cross-fade between two shapes, and do
not reach for a mask. Keep the curve type faithful: a quadratic with its control
point at the corner being cut does not look like an arc or a cubic through the
same points, and that difference is most of the character.

**Never let it settle smaller than the thing it grew out of.** A surface that
emerges from an origin — a cutout, a button, a row — must be at least as large
as that origin in every dimension, floored explicitly. Short content will
otherwise ask for less, and a panel narrower than the hole it hangs from leaves
the hardware or the trigger sticking out either side. This looks fine in a
harness with long placeholder text and wrong the moment real content is short.

**Derive radii from the origin, not from the element.** Corner radii should come
from the size of the thing it grew out of, so the shoulders stay put as it
grows, and then open up as it expands past its origin. Deriving them from the
element's own current size makes the corners swell with the content, which stops
reading as a cutout immediately. Compute them from the *current* size each frame
so they interpolate along with the box rather than jumping to their final value
on frame one.

## 4. Failure modes that render fine and still feel wrong

These all pass typecheck, pass review by eye, and are wrong.

**A keyframe with only one end stated.** The implicit end resolves to the
element's own computed value, and if the two are not the same *type* they do not
interpolate — the browser does a discrete swap. `clip-path: none` against an
`inset()` is the one that bit us: the tab appeared at full width instantly and
read as motion only because a separate opacity fade was moving. State both ends,
in the same units and the same function.

**Tearing down the element before its exit plays.** A state change that both
starts the exit and unmounts the element in the same frame produces a hard cut.
The "it's over" signal and the "remove it" signal must be separate, with the
removal downstream of the animation finishing.

**A floor that makes a morph inert.** A `min-width` or `min-height` wider than
anything the content ever needs means the "animated" dimension never actually
changes. It looks implemented and does nothing. Check the floor against real
content before believing a morph works.

**Per-frame React state.** Setting state on every frame rebuilds the subtree
under the animation, which is the stutter the CSS-owned approach exists to
avoid. Write to the node — a style property, an attribute — through a ref.

**Measuring layout inside a frame.** `getComputedStyle` and `offsetWidth` force
a style recalc. Measure when the layout changes, cache it, and let the frame
loop read the cached value.

**Animating layout properties.** `width`/`height`/`top`/`left` cost layout every
frame. Sometimes unavoidable (a container genuinely resizing) — when it is,
keep the subtree small and its children off the parent's width.

## 5. Verify by measuring, not by watching

Every overlay style renders standalone in a browser, with no Tauri and no
rebuild:

```sh
cd apps/desktop && pnpm exec vite --port 5199
open "http://localhost:5199/overlay.html?style=pill&phase=recording"
```

`?phase=`, `?style=` and `?dismiss=<ms>` drive it. Build the same kind of hatch
for any new animated surface — a way to reach every state without performing the
gesture that produces it.

Then sample the animation rather than trusting your eye. Push frames into an
array and read the shape afterwards:

```js
const seen = [];
const start = performance.now();
const tick = () => {
  seen.push([Math.round(performance.now() - start), el.offsetHeight]);
  if (performance.now() - start < 1200) requestAnimationFrame(tick);
};
requestAnimationFrame(tick);
```

What the numbers should show: movement on the very first frames, most of the
distance covered early, a long quiet tail, and — if underdamped — a peak
slightly past the target. A linear ramp means you got a duration, not a spring.
Identical values for several frames at the start means something is stalling.

Two things worth asserting in a test rather than watching: that a value the user
sees never snaps, and that an element is never `display: none` while its exit is
supposed to be running.
