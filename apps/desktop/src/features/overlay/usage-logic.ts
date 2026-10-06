import type { Quota, QuotaExtra, QuotaSample, QuotaWindow } from "@/lib/bindings";

/**
 * What the Usage view says, worked out from the plan's reading. Apart from the
 * components so the words and the chart's geometry, which can be silently
 * wrong, are testable.
 */

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

type WindowKey =
  | "fiveHour"
  | "sevenDay"
  | "sevenDayOpus"
  | "sevenDaySonnet"
  | "sevenDayOauthApps"
  | "sevenDayCowork"
  | "sevenDayOmelette";

/** Every window the usage endpoint can report, in the order the plan lists them. */
const WINDOWS: { key: WindowKey; label: string; spanMs: number }[] = [
  { key: "fiveHour", label: "5-hour window", spanMs: 5 * HOUR },
  { key: "sevenDay", label: "Week · all models", spanMs: 7 * DAY },
  { key: "sevenDayOpus", label: "Week · Opus", spanMs: 7 * DAY },
  { key: "sevenDaySonnet", label: "Week · Sonnet", spanMs: 7 * DAY },
  { key: "sevenDayOauthApps", label: "Week · connected apps", spanMs: 7 * DAY },
  { key: "sevenDayCowork", label: "Week · Routines", spanMs: 7 * DAY },
  { key: "sevenDayOmelette", label: "Week · Claude Design", spanMs: 7 * DAY },
];

export type ShownWindow = { key: WindowKey; label: string; spanMs: number; window: QuotaWindow };

/** The windows the plan reports, labelled, skipping the ones it left out. */
export function shownWindows(quota: Quota | null): ShownWindow[] {
  if (!quota) return [];
  return WINDOWS.flatMap((spec) => {
    const window = quota[spec.key];
    return window == null ? [] : [{ ...spec, window }];
  });
}

export function percent(w: QuotaWindow): number {
  return Math.min(100, Math.max(0, w.percent ?? 0));
}

/** "2 h 14 m", "3 d 4 h", "14 m": a reset countdown with the second unit, since one alone hides a lot. */
export function countdown(iso: string, now = Date.now()): string {
  const minutes = Math.floor((Date.parse(iso) - now) / 60_000);
  if (Number.isNaN(minutes)) return "";
  if (minutes < 1) return "<1 m";
  if (minutes < 60) return `${minutes} m`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} h ${minutes % 60} m`;
  return `${Math.floor(minutes / 1440)} d ${Math.floor((minutes % 1440) / 60)} h`;
}

/** How far through its window the clock is, 0…1, or null when the reset time is unknown or off. */
export function elapsedFraction(resetsAt: string | null, spanMs: number, now = Date.now()): number | null {
  if (!resetsAt) return null;
  const left = Date.parse(resetsAt) - now;
  if (Number.isNaN(left)) return null;
  return Math.min(1, Math.max(0, 1 - left / spanMs));
}

/**
 * Whether use is running ahead of the clock. The tolerance is in percentage
 * points; the first few percent of a window say nothing about the rest of it,
 * so a young window has no verdict.
 */
export function pace(used: number, elapsed: number | null): "Ahead of pace" | "Under pace" | "On pace" | "Limit reached" | null {
  if (used >= 100) return "Limit reached";
  if (elapsed == null || elapsed < 0.05) return null;
  const gap = used - elapsed * 100;
  return gap >= 10 ? "Ahead of pace" : gap <= -10 ? "Under pace" : "On pace";
}

/** Quiet under 70%, amber approaching the cap, red near it: the Usage and Home thresholds. */
export function tone(pct: number): "ink" | "warn" | "danger" {
  return pct >= 90 ? "danger" : pct >= 70 ? "warn" : "ink";
}

/**
 * The extra-usage pool as words. Amounts arrive in minor units of the plan's
 * currency (Claude Code divides by 100 to show them); two decimal places is
 * what every plan reports so far, and the response says so when it differs.
 */
export function extraSpend(extra: QuotaExtra): { spent: string; limit: string; fraction: number } | null {
  if (extra.usedCredits == null || extra.monthlyLimit == null || extra.monthlyLimit <= 0) return null;
  const money = new Intl.NumberFormat("en-US", { style: "currency", currency: extra.currency ?? "USD" });
  return {
    spent: money.format(extra.usedCredits / 100),
    limit: money.format(extra.monthlyLimit / 100),
    fraction: Math.min(1, Math.max(0, extra.usedCredits / extra.monthlyLimit)),
  };
}

export type ChartLine = {
  /** Polylines: a gap in the readings is a break, never a slope across it. */
  segments: [number, number][][];
  /** The newest point, drawn as the dot. */
  last: [number, number] | null;
  /** Readings inside the span, with their x as a 0…1 fraction, for hover. */
  points: { x: number; at: number; value: number }[];
};

/** What a chart reads off a sample: the value, and which instance of the window it was in. */
export type WindowReading = { value: number | null; resetsAt?: number | null };

/**
 * One window's readings as a line over the last `spanMs`. x and y are in a
 * `width` by `height` box, y flipped so 100% is the top.
 *
 * Readings from one window instance (same reset time) are joined across the
 * hours the view was closed, since use only climbs inside a window. A new
 * reset breaks the line, and so does a long silence when the reset is unknown
 * (`spanMs / 12`), because then a slope would be a guess.
 */
export function chartLine(
  history: QuotaSample[],
  pick: (s: QuotaSample) => WindowReading,
  spanMs: number,
  now: number,
  width: number,
  height: number,
): ChartLine {
  const points = history
    .map((s) => ({ at: s.at * 1000, ...pick(s) }))
    .filter((p): p is { at: number; value: number; resetsAt?: number | null } => p.value != null && now - p.at <= spanMs && p.at <= now)
    .map((p) => ({ ...p, x: 1 - (now - p.at) / spanMs }));
  const segments: [number, number][][] = [];
  let prev: { at: number; resetsAt?: number | null } | null = null;
  for (const p of points) {
    const xy: [number, number] = [p.x * width, height - (Math.min(100, Math.max(0, p.value)) / 100) * height];
    // To the minute: the endpoint's fractional seconds differ between reads of one window.
    const sameWindow = prev != null && p.resetsAt != null && prev.resetsAt != null && Math.floor(p.resetsAt / 60) === Math.floor(prev.resetsAt / 60);
    if (prev == null || (sameWindow ? false : p.resetsAt != null || p.at - prev.at > spanMs / 12)) segments.push([]);
    segments[segments.length - 1].push(xy);
    prev = p;
  }
  const lastSegment = segments[segments.length - 1];
  return { segments, last: lastSegment?.[lastSegment.length - 1] ?? null, points };
}
