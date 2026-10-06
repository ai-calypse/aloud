import { useEffect, useRef } from "react";
import { createSpring, type Spring } from "@/lib/spring";

/**
 * A level, 0–100: the volume bar of a whisper. It updates in place while on
 * screen — a held key sends a new value every few frames — so the fill is a live
 * spring (`lib/spring.ts`) written straight to the node: a CSS transition
 * retargeted mid-flight restarts from zero speed and the bar stutters, and
 * React state per frame would rebuild the whisper under it.
 *
 * The fill is a `scaleX` on the compositor, not a width: layout would run every
 * frame inside a panel whose size is being measured.
 */
export function LevelBar({ level, label }: { level: number; label?: string }) {
  const fill = useRef<HTMLSpanElement>(null);
  const spring = useRef<Spring | null>(null);
  // The value it first appears at: the bar is already there when the whisper
  // arrives, only later values travel.
  const first = useRef(level);

  useEffect(() => {
    const node = fill.current;
    if (!node) return;
    const draw = (value: number) => {
      node.style.transform = `scaleX(${Math.min(1, Math.max(0, value / 100))})`;
    };
    // Critically damped and quick: the bar follows the key, it does not bounce.
    const next = createSpring(draw, { damping: 1, response: 0.12 });
    next.jump(first.current);
    draw(first.current);
    spring.current = next;
    return () => {
      next.stop();
      spring.current = null;
    };
  }, []);

  useEffect(() => {
    spring.current?.to(level);
  }, [level]);

  return (
    <span
      className="level-bar"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(level)}
    >
      <span className="level-bar__fill" ref={fill} />
    </span>
  );
}
