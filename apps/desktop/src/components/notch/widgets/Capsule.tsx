import { useState } from "react";
import { cn } from "@/lib/utils";
import { litSegments, unit } from "./math";
import type { WidgetRole } from "./roles";

/**
 * A rounded bar: progress, a charge, a volume. The fill is clipped rather
 * than resized, so it moves on the compositor and keeps its round end at
 * every value (a scaled bar squashes its radius).
 *
 * With `onChange` it is scrubbable: a native range input covers it, so the
 * drag, the arrow keys and the accessibility tree are the platform's, and the
 * capsule only draws. Without it, it is a read-only `meter`.
 */
export function Capsule({
  value,
  label,
  valueText,
  onChange,
  thick = false,
  accent,
  className,
}: {
  /** 0…1. */
  value: number;
  /** What it measures: "Track position", "Battery". */
  label: string;
  /** The value in words when a percentage says too little: "1:42 of 3:20". */
  valueText?: string;
  /** Present makes it scrubbable; receives 0…1. */
  onChange?: (value: number) => void;
  /** The chunky shape of a volume slider, against a thin progress line. */
  thick?: boolean;
  accent?: WidgetRole;
  className?: string;
}) {
  const v = unit(value);
  // While a finger or pointer holds it the fill must follow at once: the
  // spring-timed clip would trail the thumb it is meant to be under.
  const [scrubbing, setScrubbing] = useState(false);
  const shape = cn(
    "relative w-full overflow-hidden rounded-full bg-widget-track",
    thick ? "h-7" : "h-1.5",
    // The ring around the capsule, drawn from the hidden input's focus.
    "has-[input:focus-visible]:outline-2 has-[input:focus-visible]:outline-offset-2 has-[input:focus-visible]:outline-solid has-[input:focus-visible]:outline-notch-focus",
    className,
  );
  const fill = (
    <div
      aria-hidden
      className={cn(
        "h-full rounded-full bg-widget-acc",
        !scrubbing && "transition-[clip-path] duration-[var(--spring-dur)] ease-[var(--spring)]",
      )}
      style={{ clipPath: `inset(0 ${100 - v * 100}% 0 0 round 999px)` }}
    />
  );

  if (!onChange) {
    return (
      <div
        role="meter"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(v * 100)}
        aria-valuetext={valueText}
        data-w-role={accent}
        className={shape}
      >
        {fill}
      </div>
    );
  }

  return (
    <div data-w-role={accent} className={shape}>
      {fill}
      <input
        type="range"
        aria-label={label}
        aria-valuetext={valueText}
        min={0}
        max={100}
        step={1}
        value={Math.round(v * 100)}
        onChange={(e) => onChange(Number(e.target.value) / 100)}
        onPointerDown={() => setScrubbing(true)}
        onPointerUp={() => setScrubbing(false)}
        onPointerCancel={() => setScrubbing(false)}
        onBlur={() => setScrubbing(false)}
        className="absolute inset-0 m-0 size-full cursor-pointer opacity-0"
      />
    </div>
  );
}

/**
 * A bar of separate segments, lit from the left: a battery's cells, a
 * five-step level. Segments are decoration; the `meter` carries the value.
 */
export function SegBar({
  value,
  segments,
  label,
  valueText,
  accent,
  className,
}: {
  /** 0…1; the lit segments are the nearest whole number to it. */
  value: number;
  segments: number;
  label: string;
  valueText?: string;
  accent?: WidgetRole;
  className?: string;
}) {
  const v = unit(value);
  const count = Math.max(1, Math.floor(segments));
  const lit = litSegments(v, count);
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      aria-valuetext={valueText}
      data-w-role={accent}
      className={cn("flex h-1.5 w-full gap-0.5", className)}
    >
      {Array.from({ length: count }, (_, i) => (
        <span
          key={i}
          aria-hidden
          className={cn(
            "flex-1 rounded-full transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
            i < lit ? "bg-widget-acc" : "bg-widget-track",
          )}
        />
      ))}
    </div>
  );
}
