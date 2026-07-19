import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * Motion primitives.
 *
 * Both of these follow the same rule: JavaScript sets a target value or
 * restarts a keyframe, and the browser's compositor runs every frame in
 * between. Nothing here animates from a rAF loop, so the motion does not
 * stutter when React is busy rendering the pane that is arriving.
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
  // The first placement must not animate — a pill that slides in from the top
  // of the window on launch looks like a bug, not a flourish.
  const placed = useRef(false);

  useLayoutEffect(() => {
    const el = pill.current;
    if (!el || !target) return;

    const top = target.offsetTop;
    const height = target.offsetHeight;

    if (!placed.current) {
      el.style.transition = "none";
      el.style.opacity = "1";
    }
    el.style.height = `${height}px`;
    el.style.transform = `translateY(${top}px)`;

    if (!placed.current) {
      // Force the no-transition frame to commit before restoring it, or the
      // browser coalesces both writes and animates the initial placement.
      void el.offsetWidth;
      el.style.transition = "";
      placed.current = true;
    }
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
