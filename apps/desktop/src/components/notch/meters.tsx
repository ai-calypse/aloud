import type { CSSProperties, ReactNode } from "react";
import { cn } from "@/lib/utils";

/** What a meter's fill says, beyond how full it is. Never the only signal:
 *  the number or word beside it carries the same news. */
export type Tone = "ink" | "ok" | "warn" | "danger";

const FILL: Record<Tone, string> = {
  ink: "bg-notch-ink",
  ok: "bg-notch-ok",
  warn: "bg-notch-warn",
  danger: "bg-notch-danger",
};

const STROKE: Record<Tone, string> = {
  ink: "stroke-notch-ink",
  ok: "stroke-notch-ok",
  warn: "stroke-notch-warn",
  danger: "stroke-notch-danger",
};

/** 0…1, whatever arrives. A NaN draws empty rather than a full bar. */
function unit(value: number): number {
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

/**
 * A horizontal meter: usage, progress, a download. The fill is clipped
 * rather than resized, so it moves on the compositor and keeps its rounded
 * end at every value (a scaled bar squashes its radius).
 */
export function Bar({
  value,
  label,
  tone = "ink",
  className,
}: {
  value: number;
  /** What it measures, for assistive tech: "5-hour window". */
  label: string;
  tone?: Tone;
  className?: string;
}) {
  const v = unit(value);
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v * 100)}
      className={cn("h-1.5 overflow-hidden rounded-full bg-notch-track", className)}
    >
      <div
        className={cn(
          "h-full rounded-full transition-[clip-path] duration-[var(--spring-dur)] ease-[var(--spring)]",
          FILL[tone],
        )}
        style={{ clipPath: `inset(0 ${100 - v * 100}% 0 0 round 999px)` }}
      />
    </div>
  );
}

/**
 * A circular meter with its lead in the middle: the timer's ring. An SVG
 * stroke, not a conic gradient — the end cap is round and the track and fill
 * are tokens rather than baked stops.
 */
export function Ring({
  value,
  label,
  size = 150,
  stroke = 9,
  tone = "ink",
  children,
  className,
}: {
  value: number;
  label: string;
  size?: number;
  stroke?: number;
  tone?: Tone;
  children?: ReactNode;
  className?: string;
}) {
  const r = (size - stroke) / 2;
  const circumference = 2 * Math.PI * r;
  return (
    <div
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(unit(value) * 100)}
      className={cn("relative grid shrink-0 place-items-center", className)}
      style={{ width: size, height: size }}
    >
      <svg className="absolute inset-0 -rotate-90" viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" strokeWidth={stroke} className="stroke-notch-track" />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          className={cn(
            "transition-[stroke-dashoffset] duration-[var(--spring-dur)] ease-[var(--spring)]",
            STROKE[tone],
          )}
          style={{ strokeDashoffset: circumference * (1 - unit(value)) } as CSSProperties}
        />
      </svg>
      <div className="relative text-center">{children}</div>
    </div>
  );
}
