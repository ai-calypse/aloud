import { useEffect, useRef, type ReactNode } from "react";
import { createSpring, type Spring } from "@/lib/spring";
import { cn } from "@/lib/utils";
import { dashOffset, ringGeometry, stackGeometry, unit } from "./math";
import type { WidgetRole } from "./roles";

/**
 * How the arc reaches a new value. `spring` for a value the user or the
 * world can change at any moment (a battery, a CPU reading): a live spring
 * (lib/spring.ts) so a second change mid-flight keeps its momentum. `drain`
 * for a fixed ride the clock owns (the focus timer): a linear CSS transition
 * of one tick. `none` jumps.
 */
export type RingMotion = "spring" | "drain" | "none";

/**
 * A ring: a bright arc over a track of the same hue, with the lead in the
 * middle. One stroked circle, so the end cap is round and the colours are the
 * widget's accent (`data-w-role`), not baked stops. Never the only carrier of
 * a value: it is a `meter`, and the number or word beside it says the same.
 */
export function Ring({
  value,
  label,
  valueText,
  size = 52,
  stroke = 9,
  accent,
  motion = "spring",
  children,
  className,
}: {
  /** 0…1. */
  value: number;
  /** What it measures, for assistive tech: "Battery". */
  label: string;
  /** The value in words when a percentage says too little: "19 of 25 minutes". */
  valueText?: string;
  size?: number;
  stroke?: number;
  /** Its role's accent; omitted, it inherits the frame's. */
  accent?: WidgetRole;
  motion?: RingMotion;
  children?: ReactNode;
  className?: string;
}) {
  const { r, circumference } = ringGeometry(size, stroke);
  const v = unit(value);
  const arc = useRef<SVGCircleElement>(null);
  const spring = useRef<Spring | null>(null);
  const latest = useRef({ v, circumference });
  latest.current = { v, circumference };

  // Built once per ring, and it grows from empty: the ring filling is its
  // entrance. Retargeting below goes through the same spring, so it never
  // restarts.
  useEffect(() => {
    const el = arc.current;
    if (!el || motion !== "spring") return;
    const s = createSpring(
      (p) => {
        el.style.strokeDashoffset = String(dashOffset(latest.current.circumference, p));
      },
      { damping: 1, response: 0.5 },
    );
    spring.current = s;
    s.jump(0);
    s.to(latest.current.v);
    return () => {
      s.stop();
      spring.current = null;
    };
  }, [motion]);

  useEffect(() => {
    spring.current?.to(v);
  }, [v]);

  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      aria-valuetext={valueText}
      data-w-role={accent}
      className={cn("relative grid shrink-0 place-items-center", className)}
      style={{ width: size, height: size }}
    >
      <svg className="absolute inset-0 -rotate-90" viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          className="stroke-widget-track"
        />
        <circle
          ref={arc}
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          // A round cap draws a dot even for a zero-length arc.
          opacity={v > 0 ? 1 : 0}
          // The spring owns the offset after mount, so React must not rewrite
          // it: an attribute is only touched when it changes, a style prop on
          // every render. Spring mode starts empty and the effect fills it.
          {...(motion === "spring"
            ? { strokeDashoffset: circumference }
            : { style: { strokeDashoffset: dashOffset(circumference, v) } })}
          data-motion={motion}
          className="notch-ring__arc stroke-widget-ring"
        />
      </svg>
      {children && <div className="relative text-center">{children}</div>}
    </div>
  );
}

/**
 * Three concentric rings, the Fitness look: Move, Exercise, Stand outermost
 * to innermost, each its own role. `rings` is outermost first; each ring is
 * a meter of its own inside one group, so a screen reader hears all three.
 */
export function RingStack({
  rings,
  label,
  size = 52,
  motion,
  className,
}: {
  rings: { value: number; label: string; accent: WidgetRole; valueText?: string }[];
  label: string;
  size?: number;
  motion?: RingMotion;
  className?: string;
}) {
  const geometry = stackGeometry(size, rings.length);
  return (
    <div
      role="group"
      aria-label={label}
      className={cn("relative shrink-0", className)}
      style={{ width: size, height: size }}
    >
      {rings.map((ring, i) => (
        <Ring
          key={ring.label}
          {...ring}
          size={geometry[i].size}
          stroke={geometry[i].stroke}
          motion={motion}
          className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
        />
      ))}
    </div>
  );
}
