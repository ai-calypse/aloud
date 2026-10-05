import { useState } from "react";
import type { QuotaSample } from "@/lib/bindings";
import { Label } from "@/components/notch/list";
import { chartLine, type WindowReading } from "./usage-logic";

const W = 300;
const H = 56;

/**
 * One window's readings over time: a thin line on a recessive grid, the newest
 * reading as a dot, and a crosshair that puts the reading under the pointer in
 * the header (so the tooltip never clips against the panel's edge).
 *
 * Drawn in a fixed viewBox stretched to the column, with non-scaling strokes so
 * lines and the dot (a zero-length round-capped stroke) keep their weight at
 * any width. The line is ink: one series needs no hue, and the severity colour
 * stays on the meters, where the number beside them backs it up.
 *
 * ponytail: no table view; the limits list beside it is the current reading in
 * text and the chart only adds its history. A `<table>` of the samples is the
 * upgrade path if the chart ever carries something the list does not.
 */
export function UsageChart({
  title,
  history,
  pick,
  spanMs,
  spanLabel,
  now = Date.now(),
}: {
  title: string;
  history: QuotaSample[];
  pick: (s: QuotaSample) => WindowReading;
  spanMs: number;
  /** The left end of the x axis: "24 h ago". */
  spanLabel: string;
  now?: number;
}) {
  const line = chartLine(history, pick, spanMs, now, W, H);
  const [hover, setHover] = useState<number | null>(null);
  const latest = line.points[line.points.length - 1];
  const shown = hover == null ? latest : line.points[hover];
  const sparse = line.points.length < 2;

  const aim = (e: React.PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const x = (e.clientX - box.left) / box.width;
    let best = -1;
    line.points.forEach((p, i) => {
      if (best < 0 || Math.abs(p.x - x) < Math.abs(line.points[best].x - x)) best = i;
    });
    setHover(best < 0 ? null : best);
  };

  return (
    <div>
      <div className="mb-1.5 flex items-baseline justify-between gap-2 font-notch">
        <Label className="m-0">{title}</Label>
        {shown && (
          <span className="font-notch-mono text-notch-meta whitespace-nowrap text-notch-ink-2 tabular-nums">
            {Math.round(shown.value)}%{hover == null ? " now" : ` · ${when(shown.at)}`}
          </span>
        )}
      </div>
      <div className="grid grid-cols-[2rem_1fr] gap-x-1.5">
        <div className="flex flex-col justify-between text-right font-notch text-notch-label leading-none text-notch-ink-3" aria-hidden>
          <span>100%</span>
          <span>0</span>
        </div>
        <div
          role="img"
          aria-label={
            latest
              ? `${title}: ${Math.round(latest.value)}% now, ${line.points.length} readings`
              : `${title}: no readings yet`
          }
          className="relative h-14"
          onPointerMove={sparse ? undefined : aim}
          onPointerLeave={() => setHover(null)}
        >
          <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="absolute inset-0 size-full overflow-visible" aria-hidden>
            {[0, 0.5, 1].map((f) => (
              <line key={f} x1={0} x2={W} y1={H * f} y2={H * f} strokeWidth={1} vectorEffect="non-scaling-stroke" className="stroke-notch-sep" />
            ))}
            {line.segments.map((seg, i) => {
              const path = seg.map(([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`).join("L");
              return (
                <g key={i}>
                  {seg.length > 1 && (
                    <path d={`M${path}L${seg[seg.length - 1][0]} ${H}L${seg[0][0]} ${H}Z`} className="fill-notch-ink/10" />
                  )}
                  <path
                    d={`M${path}${seg.length === 1 ? "h0.01" : ""}`}
                    fill="none"
                    strokeWidth={seg.length === 1 ? 5 : 1.5}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    vectorEffect="non-scaling-stroke"
                    className="stroke-notch-ink"
                  />
                </g>
              );
            })}
            {line.last && hover == null && (
              <path d={`M${line.last[0]} ${line.last[1]}h0.01`} strokeWidth={6} strokeLinecap="round" vectorEffect="non-scaling-stroke" className="stroke-notch-ink" />
            )}
            {hover != null && shown && (
              <>
                <line x1={shown.x * W} x2={shown.x * W} y1={0} y2={H} strokeWidth={1} vectorEffect="non-scaling-stroke" className="stroke-notch-ink-3" />
                <path d={`M${shown.x * W} ${H - (shown.value / 100) * H}h0.01`} strokeWidth={6} strokeLinecap="round" vectorEffect="non-scaling-stroke" className="stroke-notch-ink" />
              </>
            )}
          </svg>
          {sparse && (
            <p className="absolute inset-0 m-0 grid place-items-center bg-notch/80 px-3 text-center font-notch text-notch-meta text-pretty text-notch-ink-3">
              Aloud draws this from the readings it takes while Usage is open. It fills in as you come back.
            </p>
          )}
        </div>
        <span />
        <div className="mt-1 flex justify-between font-notch text-notch-label text-notch-ink-3" aria-hidden>
          <span>{spanLabel}</span>
          <span>now</span>
        </div>
      </div>
    </div>
  );
}

function when(ms: number): string {
  return new Date(ms).toLocaleString([], { weekday: "short", hour: "numeric", minute: "2-digit" });
}
