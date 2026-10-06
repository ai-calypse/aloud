import type { Event as CalendarEvent, Reminder, SeriesPoint } from "@/lib/bindings";
import type { EventTone } from "@/components/notch/widgets";
import { nextEvent, overdue } from "@/lib/today";

/**
 * What the Today widgets say, worked out from the Today hooks' data. Apart from the components so the parts that can silently be wrong (a
 * month starting on the wrong weekday, an event drawn an hour off, a range
 * bar past its track) are testable on their own.
 */

/** The hour axis of a calendar tile: seven hours is what 354pt holds at a readable event height. */
export const GRID_HOURS = 7;

const HOUR = 3_600_000;

/** Today's and tomorrow's live events, as the calm Calendar groups them. */
export function splitAgenda(items: CalendarEvent[], now: number) {
  const live = items.filter((e) => e.end > now);
  const midnight = new Date(now).setHours(24, 0, 0, 0);
  return {
    live,
    today: live.filter((e) => e.start < midnight),
    tomorrow: live.filter((e) => e.start >= midnight),
    // What the island is showing too.
    next: nextEvent(live, now),
  };
}

/**
 * A calendar's colour. The read carries only the calendar's name, so the hue
 * is a stable pick from the four the card knows: the same calendar is always
 * the same colour, and the words carry the event.
 */
const TONES: EventTone[] = ["green", "yellow", "blue", "red"];
export function toneFor(calendar: string): EventTone {
  let h = 0;
  for (const ch of calendar) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return TONES[h % TONES.length];
}

/** `3 more events`, `1 more event`. */
export function moreLabel(n: number): string {
  return `${n} more event${n === 1 ? "" : "s"}`;
}

export type Placed = {
  event: CalendarEvent;
  /** 0…1 down the window. */
  top: number;
  height: number;
  /** Which of `lanes` side-by-side columns it takes, for events that overlap. */
  lane: number;
  lanes: number;
};

/**
 * The hour a column's axis starts at. A column starts where its attention is:
 * the current hour for today, the first event for another day. If nothing
 * lands in that window the axis jumps to the first thing after it, so a tile
 * never opens on seven empty hours while an event waits just past the edge.
 */
export function windowStart(timed: CalendarEvent[], dayStart: number, anchorHour: number): number {
  const clamp = (h: number) => Math.min(24 - GRID_HOURS, Math.max(0, h));
  const hourOf = (ms: number) => Math.floor((ms - dayStart) / HOUR);
  const start = clamp(anchorHour);
  const inWindow = (from: number) =>
    timed.some((e) => e.end > dayStart + from * HOUR && e.start < dayStart + (from + GRID_HOURS) * HOUR);
  if (inWindow(start)) return start;
  const later = timed
    .filter((e) => e.start >= dayStart + (start + GRID_HOURS) * HOUR)
    .sort((a, b) => a.start - b.start)[0];
  return later ? clamp(hourOf(later.start)) : start;
}

/**
 * One day's timed events laid on an hour axis: what fits the window, clipped
 * to it, side by side where they overlap, and the rest handed back as `hidden`
 * for the "N more events" line. All-day events are not on the axis.
 */
export function layoutDay(
  events: CalendarEvent[],
  dayStart: number,
  startHour: number,
): { placed: Placed[]; hidden: CalendarEvent[] } {
  const from = dayStart + startHour * HOUR;
  const to = from + GRID_HOURS * HOUR;
  const timed = events.filter((e) => !e.allDay).sort((a, b) => a.start - b.start || a.end - b.end);
  const visible = timed.filter((e) => e.end > from && e.start < to);
  const hidden = timed.filter((e) => !visible.includes(e));

  // Greedy lanes within a cluster of overlapping events.
  const placed: Placed[] = [];
  let cluster: Placed[] = [];
  let clusterEnd = -Infinity;
  const close = () => {
    const lanes = Math.max(1, ...cluster.map((p) => p.lane + 1));
    for (const p of cluster) p.lanes = lanes;
    cluster = [];
  };
  for (const event of visible) {
    if (event.start >= clusterEnd) close();
    const taken = new Set(cluster.filter((p) => p.event.end > event.start).map((p) => p.lane));
    let lane = 0;
    while (taken.has(lane)) lane++;
    const top = (Math.max(event.start, from) - from) / (to - from);
    const bottom = (Math.min(event.end, to) - from) / (to - from);
    const p = { event, top, height: Math.max(bottom - top, 0), lane, lanes: 1 };
    cluster.push(p);
    placed.push(p);
    clusterEnd = Math.max(clusterEnd, event.end);
  }
  close();
  return { placed, hidden };
}

/** 0…1 down the window for `now`, or null when the red line has nowhere to be. */
export function nowLine(now: number, dayStart: number, startHour: number): number | null {
  const from = dayStart + startHour * HOUR;
  const at = (now - from) / (GRID_HOURS * HOUR);
  return at >= 0 && at <= 1 ? at : null;
}

/** `9`, `12`, `1`: the hour on the axis in the clock's own style (24-hour clocks keep 13). */
export function hourLabel(hour: number, locale?: string): string {
  const text = new Date(2000, 0, 1, hour).toLocaleTimeString(locale, { hour: "numeric" });
  // "9 AM" -> "9": the axis is narrow and the column's header says the day.
  return /\d/.test(text) ? text.replace(/\s?[AP]M$/i, "") : text;
}

/** The first weekday of the locale's week as a JS day (0 is Sunday); Sunday when the engine cannot say. */
export function firstWeekday(locale?: string): number {
  try {
    const l = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const info = l.getWeekInfo?.() ?? l.weekInfo;
    // `firstDay` counts Monday as 1 and Sunday as 7.
    if (info) return info.firstDay % 7;
  } catch {
    /* fall through */
  }
  return 0;
}

export type Month = {
  /** `October 2026`. */
  title: string;
  /** One letter per column, in the week's order. */
  weekdays: string[];
  /** Weeks of day numbers, `null` where the month is not. */
  weeks: (number | null)[][];
  /** The day of the month that is today. */
  today: number;
};

/** The month `now` is in, as a grid that begins on `weekStart`. */
export function monthGrid(now: number, weekStart = firstWeekday(), locale?: string): Month {
  const date = new Date(now);
  const year = date.getFullYear();
  const month = date.getMonth();
  const days = new Date(year, month + 1, 0).getDate();
  const lead = (new Date(year, month, 1).getDay() - weekStart + 7) % 7;
  const cells: (number | null)[] = [
    ...Array<null>(lead).fill(null),
    ...Array.from({ length: days }, (_, i) => i + 1),
  ];
  while (cells.length % 7 !== 0) cells.push(null);
  const weeks = Array.from({ length: cells.length / 7 }, (_, w) => cells.slice(w * 7, w * 7 + 7));
  // 2000-01-02 was a Sunday, so day `2 + i` is the i-th weekday from Sunday.
  const weekdays = Array.from({ length: 7 }, (_, i) =>
    new Date(2000, 0, 2 + ((weekStart + i) % 7)).toLocaleDateString(locale, { weekday: "narrow" }),
  );
  return {
    title: date.toLocaleDateString(locale, { month: "long", year: "numeric" }),
    weekdays,
    weeks,
    today: date.getDate(),
  };
}

/** Local midnight of the day `ms` falls on. */
export const dayStartOf = (ms: number): number => new Date(ms).setHours(0, 0, 0, 0);

/** The local midnight `n` days from `dayStart`, by the calendar: a day with a clock change is not 24 hours. */
export function shiftDay(dayStart: number, n: number): number {
  const d = new Date(dayStart);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n).getTime();
}

/** The first of the month `n` months from the one `ms` is in. */
export function shiftMonth(ms: number, n: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
}

/** Whether two instants are on the same local day. */
export const sameDay = (a: number, b: number): boolean => dayStartOf(a) === dayStartOf(b);

/**
 * What the month card's arrow keys do to a selected date: a day either way,
 * a week up or down, and Escape back to today. `null` means "not a key for
 * this", so the panel's own handling goes on.
 */
export function moveSelection(key: string, selected: number): number | null {
  switch (key) {
    case "ArrowLeft":
      return shiftDay(selected, -1);
    case "ArrowRight":
      return shiftDay(selected, 1);
    case "ArrowUp":
      return shiftDay(selected, -7);
    case "ArrowDown":
      return shiftDay(selected, 7);
    default:
      return null;
  }
}

/**
 * One day's events out of a longer read (today and tomorrow, which are already
 * in hand), without asking Calendar again: everything that overlaps the day,
 * all-day first.
 */
export function eventsOnDay(items: CalendarEvent[], dayStart: number): CalendarEvent[] {
  const next = shiftDay(dayStart, 1);
  return items
    .filter((e) => e.start < next && e.end > dayStart)
    .sort((a, b) => Number(!a.allDay) - Number(!b.allDay) || a.start - b.start);
}

/** The reminders tile's two numbers: what is due, and what has slipped. */
export function reminderCounts(items: Reminder[], now: number): { total: number; late: number; due: number } {
  const late = items.filter((r) => overdue(r.due, r.allDay, now)).length;
  return { total: items.length, late, due: items.length - late };
}

/** `Today`, `Mon`: a forecast day's name, from the place's own date so a far timezone cannot shift it. */
export function dayLabel(day: string, first: boolean, locale?: string): string {
  if (first) return "Today";
  return new Date(`${day}T12:00:00`).toLocaleDateString(locale, { weekday: "short" });
}

/**
 * Where a day's low-to-high bar sits on the week's own range, as fractions of
 * the track. Null temperatures draw no bar rather than a wrong one, and a flat
 * week draws a full one rather than dividing by zero.
 */
export function rangeBar(
  low: number | null,
  high: number | null,
  weekLow: number,
  weekHigh: number,
): { left: number; width: number } | null {
  if (low === null || high === null) return null;
  const span = weekHigh - weekLow;
  if (span <= 0) return { left: 0, width: 1 };
  // A one-degree day still gets a bar you can see, and never one off the track.
  const width = Math.max((high - low) / span, 0.06);
  return { left: Math.min((low - weekLow) / span, 1 - width), width };
}

/** The week's own coldest low and hottest high, ignoring days with no reading. */
export function weekRange(days: { high: number | null; low: number | null }[]): { low: number; high: number } | null {
  const lows = days.flatMap((d) => (d.low === null ? [] : [d.low]));
  const highs = days.flatMap((d) => (d.high === null ? [] : [d.high]));
  if (lows.length === 0 || highs.length === 0) return null;
  return { low: Math.min(...lows), high: Math.max(...highs) };
}

/** Up, down or flat by the same dead zone the calm list uses, so a 0.03% wobble is not a colour. */
export function moveOf(changePct: number | null): "up" | "down" | "flat" | null {
  if (changePct === null) return null;
  return changePct > 0.05 ? "up" : changePct < -0.05 ? "down" : "flat";
}

export type DayChart = {
  /** The line, as an SVG path. */
  line: string;
  /** Yesterday's close as a height in the box, or null when the feed gave none. */
  baseline: number | null;
  /** The last bar: where the line ends and the dot goes. */
  end: [number, number];
  /** Which side of yesterday's close the day ended on. */
  move: "up" | "down" | "flat";
};

/**
 * A day's line in a `width` x `height` box. The y range always includes
 * yesterday's close, so the baseline is on the chart and the line's distance
 * from it is true to scale (a flat day hugs it, a big day crosses the box).
 * `x` is the backend's fraction of the session, so a day still in progress
 * stops short of the right edge instead of stretching to fill it.
 *
 * Null when there is no line to draw: a point is a dot, and Rust's NaN
 * arrives as null, which is dropped rather than drawn as zero.
 */
export function dayChart(
  series: SeriesPoint[],
  previousClose: number | null,
  width: number,
  height: number,
  pad = 3,
): DayChart | null {
  const pts = series.filter(
    (p): p is { x: number; y: number } => Number.isFinite(p.x) && Number.isFinite(p.y),
  );
  if (pts.length < 2) return null;
  const base = previousClose !== null && Number.isFinite(previousClose) ? previousClose : null;
  const ys = base === null ? pts.map((p) => p.y) : [...pts.map((p) => p.y), base];
  const min = Math.min(...ys);
  const span = Math.max(...ys) - min;
  const x = (v: number) => pad + Math.min(1, Math.max(0, v)) * (width - 2 * pad);
  const y = (v: number) => (span === 0 ? height / 2 : height - pad - ((v - min) / span) * (height - 2 * pad));
  const last = pts[pts.length - 1];
  const ref = base ?? pts[0].y;
  return {
    line: pts.map((p, i) => `${i === 0 ? "M" : "L"}${x(p.x).toFixed(1)} ${y(p.y).toFixed(1)}`).join(" "),
    baseline: base === null ? null : y(base),
    end: [x(last.x), y(last.y)],
    move: moveOf(ref === 0 ? null : ((last.y - ref) / ref) * 100) ?? "flat",
  };
}
