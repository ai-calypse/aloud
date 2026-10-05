import type { Event as CalendarEvent, Reminder } from "@/lib/bindings";
import type { EventTone } from "@/components/notch/widgets";
import { nextEvent, overdue } from "@/lib/today";

/**
 * What the Today widgets say, worked out from the same data the calm views
 * read. Apart from the components so the parts that can silently be wrong (a
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
