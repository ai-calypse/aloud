import { cn } from "@/lib/utils";
import { sparkPath, sparkPoints } from "./math";
import type { WidgetRole } from "./roles";

/**
 * A sparkline: one stroke, no axes, no fill unless asked for. It is a picture of a trend, so
 * it is an `img` whose label says what the trend is and what it did, never
 * the only place a number lives.
 */
export function Spark({
  values,
  label,
  width = 120,
  height = 32,
  accent,
  fill = false,
  className,
}: {
  values: number[];
  /** "CPU over the last minute, steady". */
  label: string;
  width?: number;
  height?: number;
  accent?: WidgetRole;
  /** Shades under the line and ends it on a dot, as Calm's weather tile does. */
  fill?: boolean;
  className?: string;
}) {
  const d = sparkPath(values, width, height);
  const points = fill ? sparkPoints(values, width, height) : [];
  const end = points[points.length - 1];
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
      {d && fill && end && (
        <>
          <path d={`${d} L${width} ${height} L0 ${height}Z`} className="fill-widget-acc/15" />
          <circle cx={end[0]} cy={end[1]} r={3} className="fill-widget-acc" />
        </>
      )}
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
