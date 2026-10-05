import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { createSpring, type Spring } from "@/lib/spring";
import { cn } from "@/lib/utils";

/**
 * The selection chip that slides between rows of a list — or, with
 * `axis="x"`, between the segments of a tab strip. One element behind the
 * items, not a background per item — those can only cross-fade.
 *
 * The exception to this app's compositor-owns-every-frame rule, because the
 * user can redirect it mid-travel and a CSS transition cannot carry velocity
 * through that (see lib/spring.ts).
 *
 * Position is measured from the active item, not `index × size`: the
 * sidebar's group divider means the rows are not evenly pitched, and tab
 * labels are not one width.
 */
export function SlidingPill({
  /** Element the pill should currently sit behind. */
  target,
  /** Re-measure when this changes (the active id, plus anything resizing). */
  deps,
  className,
  /**
   * `y` travels a vertical list and takes the row's height; `x` travels a
   * horizontal strip and springs its width too, since segments differ. On
   * `x` the pill's vertical geometry is the caller's (`className`): a
   * segment's thumb and a tab's underline sit at different heights.
   */
  axis = "y",
  /** Pulls an `x` pill in from both ends of its target — an underline that
   *  stops short of the label's padding. */
  inset = 0,
  /** `accent` is the sidebar's filled chip; `bare` draws nothing of its own,
   *  so `className` owns the look (the notch's thumb and underline). */
  tone = "accent",
}: {
  target: HTMLElement | null;
  deps: unknown[];
  className?: string;
  axis?: "x" | "y";
  inset?: number;
  tone?: "accent" | "bare";
}) {
  const pill = useRef<HTMLDivElement>(null);
  const springs = useRef<{ position: Spring; size: Spring } | null>(null);
  // The first placement must not animate — sliding in from the top of the
  // window on launch looks like a bug.
  const placed = useRef(false);

  useLayoutEffect(() => {
    const el = pill.current;
    if (!el) return;

    // Critically damped: sailing past the row it is meant to mark reads as a
    // miss. Overshoot is reserved for motion the user threw.
    const feel = { damping: 1, response: 0.3 };
    const position = createSpring((v) => {
      el.style.transform = axis === "x" ? `translateX(${v}px)` : `translateY(${v}px)`;
    }, feel);
    // Only an `x` pill springs its size; a `y` pill's height is written below.
    const size = createSpring((v) => {
      el.style.width = `${v}px`;
    }, feel);
    springs.current = { position, size };
    placed.current = false;
    return () => {
      position.stop();
      size.stop();
    };
  }, [axis]);

  useLayoutEffect(() => {
    const el = pill.current;
    const driver = springs.current;
    if (!el || !driver || !target) return;

    const position = axis === "x" ? target.offsetLeft + inset : target.offsetTop;
    const width = target.offsetWidth - inset * 2;
    // Written straight through, not sprung: sidebar rows are all one height,
    // so this never animates. Give them different heights and it needs one.
    if (axis === "y") el.style.height = `${target.offsetHeight}px`;

    if (!placed.current) {
      el.style.opacity = "1";
      driver.position.jump(position);
      if (axis === "x") driver.size.jump(width);
      placed.current = true;
      return;
    }

    // Retarget, keeping current velocity: a click mid-travel bends the move.
    driver.position.to(position);
    if (axis === "x") driver.size.to(width);
    // `target` is load-bearing in this list: it arrives from a callback ref one
    // render *after* the id changes, so keying only on the caller's deps left
    // the pill unpositioned on first paint.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target, axis, inset, ...deps]);

  return (
    <div
      ref={pill}
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute opacity-0",
        axis === "x" ? "left-0" : "inset-x-0 top-0",
        tone === "accent" ? "motion-pill" : "will-change-transform",
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
