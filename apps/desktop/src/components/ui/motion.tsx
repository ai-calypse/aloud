import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { createSpring, type Spring } from "@/lib/spring";
import { cn } from "@/lib/utils";

/**
 * The selection chip that slides between rows of a list. One element behind
 * the rows, not a background per row — those can only cross-fade.
 *
 * The exception to this app's compositor-owns-every-frame rule, because the
 * user can redirect it mid-travel and a CSS transition cannot carry velocity
 * through that (see lib/spring.ts).
 *
 * Position is measured from the active row, not `index × rowHeight`: the
 * sidebar's group divider means the rows are not evenly pitched.
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
  // The first placement must not animate — sliding in from the top of the
  // window on launch looks like a bug.
  const placed = useRef(false);

  useLayoutEffect(() => {
    const el = pill.current;
    if (!el) return;

    // Critically damped: sailing past the row it is meant to mark reads as a
    // miss. Overshoot is reserved for motion the user threw.
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

    // Written straight through, not sprung: sidebar rows are all one height,
    // so this never animates. Give them different heights and it needs one.
    el.style.height = `${target.offsetHeight}px`;

    if (!placed.current) {
      el.style.opacity = "1";
      driver.jump(target.offsetTop);
      placed.current = true;
      return;
    }

    // Retarget, keeping current velocity: a click mid-travel bends the move.
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
 * Re-plays a "materialize" entrance whenever `index` changes, entering from
 * the direction the user moved through the list.
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

    const direction = index > previous.current ? 1 : -1;
    previous.current = index;
    el.style.setProperty("--materialize-from", `${direction * 7}px`);

    // Restart the keyframe: re-assigning the same animation name is a no-op
    // unless it is cleared and the removal flushed, which offsetWidth forces.
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
