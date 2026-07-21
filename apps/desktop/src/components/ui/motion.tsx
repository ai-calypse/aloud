import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { createSpring, type Spring } from "@/lib/spring";
import { cn } from "@/lib/utils";

/**
 * Motion primitives.
 *
 * The default rule in this app is that JavaScript sets a target value or
 * restarts a keyframe and the compositor runs every frame in between, so
 * motion does not stutter while React renders. `Materialize` below is exactly
 * that.
 *
 * `SlidingPill` is the deliberate exception, and the reason is interruption:
 * see lib/spring.ts. It is the only element here a user can redirect while it
 * is still moving, and a CSS transition cannot carry velocity through that.
 */

/**
 * The selection chip that slides between rows of a list.
 *
 * One element behind the rows rather than a background on each: a background
 * per row can only cross-fade, which reads as two separate things blinking.
 * A single chip that travels reads as one object moving, which is what the
 * user's mental model already is.
 *
 * Position is measured from the active row rather than computed as
 * `index × rowHeight`, because the sidebar puts a divider between its two
 * groups — the rows are not evenly pitched, and a constant would drift by the
 * divider's height for every row below it.
 */
export function SlidingPill({
  /** Element the pill should currently sit behind. */
  target,
  /** Re-measure when this changes (the active id, plus anything resizing). */
  deps,
  className,
}: {
  target: HTMLElement | null;
  deps: unknown[];
  className?: string;
}) {
  const pill = useRef<HTMLDivElement>(null);
  const spring = useRef<Spring | null>(null);
  // The first placement must not animate — a pill that slides in from the top
  // of the window on launch looks like a bug, not a flourish.
  const placed = useRef(false);

  useLayoutEffect(() => {
    const el = pill.current;
    if (!el) return;

    // Critically damped: this chip lands on a row the user picked, and a chip
    // that sails past the row it is meant to mark reads as a miss, not as
    // physics. Overshoot is reserved for motion the user threw.
    const driver = createSpring(
      (y) => {
        el.style.transform = `translateY(${y}px)`;
      },
      { damping: 1, response: 0.3 },
    );
    spring.current = driver;
    return () => driver.stop();
  }, []);

  useLayoutEffect(() => {
    const el = pill.current;
    const driver = spring.current;
    if (!el || !driver || !target) return;

    // Height is written straight through rather than sprung: every row in the
    // sidebar is the same height, so this never actually animates, and a
    // second spring on a layout property would cost a reflow a frame to do
    // nothing. Give the rows different heights and this needs its own spring.
    el.style.height = `${target.offsetHeight}px`;

    if (!placed.current) {
      el.style.opacity = "1";
      driver.jump(target.offsetTop);
      placed.current = true;
      return;
    }

    // Retarget. Whatever speed the chip already has is kept, so clicking a
    // second row mid-travel bends the move instead of stalling and restarting.
    driver.to(target.offsetTop);
    // `target` is load-bearing in this list: it arrives from a callback ref one
    // render *after* the id changes, so keying only on the caller's deps left
    // the pill unpositioned on first paint.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, ...deps]);

  return (
    <div
      ref={pill}
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute inset-x-0 top-0 opacity-0",
        "motion-pill",
        className,
      )}
    />
  );
}

/**
 * Re-plays a "materialize" entrance whenever `index` changes.
 *
 * A plain fade tells you the content changed. This tells you *which way you
 * moved*: content enters from below when you go down the list and from above
 * when you go up, so the pane's motion agrees with the direction your eye
 * just travelled. Blur is what makes it read as arriving rather than sliding —
 * an out-of-focus thing coming into focus, not a card on a rail.
 */
export function Materialize({
  index,
  children,
  className,
}: {
  /** Position of the current view in its list; sign of the delta gives direction. */
  index: number;
  children: ReactNode;
  className?: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const previous = useRef(index);

  useEffect(() => {
    const el = host.current;
    if (!el || previous.current === index) return;

    // Enter from the direction travelled: down the list → up from below.
    const direction = index > previous.current ? 1 : -1;
    previous.current = index;
    el.style.setProperty("--materialize-from", `${direction * 7}px`);

    // Restart the keyframe. Re-assigning the same animation name is a no-op
    // unless the animation is cleared and the removal is flushed first, which
    // is what reading offsetWidth forces.
    el.style.animation = "none";
    void el.offsetWidth;
    el.style.animation = "";
  }, [index]);

  return (
    <div ref={host} className={cn("motion-materialize", className)}>
      {children}
    </div>
  );
}
