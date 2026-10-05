import { cn } from "@/lib/utils";
import { sparkPath } from "./math";
import type { WidgetRole } from "./roles";

/**
 * A sparkline: one stroke, no axes, no fill. It is a picture of a trend, so
 * it is an `img` whose label says what the trend is and what it did, never
 * the only place a number lives.
 */
export function Spark({
  values,
  label,
  width = 120,
  height = 32,
  accent,
  className,
}: {
  values: number[];
  /** "CPU over the last minute, steady". */
  label: string;
  width?: number;
  height?: number;
  accent?: WidgetRole;
  className?: string;
}) {
  const d = sparkPath(values, width, height);
  return (
    <svg
      role="img"
      aria-label={label}
      data-w-role={accent}
      viewBox={`0 0 ${width} ${height}`}
      width={width}
      height={height}
      className={cn("shrink-0 overflow-visible", className)}
    >
      {d && (
        <path
          d={d}
          fill="none"
          strokeWidth={1.6}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="stroke-widget-ring"
        />
      )}
    </svg>
  );
}
