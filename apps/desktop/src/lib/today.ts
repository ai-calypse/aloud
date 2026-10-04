import type { Event as CalendarEvent, MarketState, Ticker } from "@/lib/bindings";

/**
 * Fahrenheit for the few regions that use it. The locale's region, not the
 * language: `en-GB` and `en-CA` are English and Celsius.
 */
export function usesFahrenheit(locale: string): boolean {
  let region: string | undefined;
  try {
    region = new Intl.Locale(locale).region;
  } catch {
    return false;
  }
  return region === "US" || region === "LR" || region === "MM";
}

/**
 * `21°`, `-3°`. Rounds, and never shows `-0°`. Rust's f64 reaches the
 * webview as `number | null` (a NaN serialises to null), hence the dash.
 */
export function degrees(value: number | null): string {
  if (value === null) return "–";
  const n = Math.round(value);
  return `${n === 0 ? 0 : n}°`;
}

/** What a quote's state says beside its price; nothing while it is live. */
export function marketLabel(t: Pick<Ticker, "state" | "delayed">): string {
  if (t.delayed) return "delayed";
  const labels: Record<MarketState, string> = {
    open: "",
    pre: "pre-market",
    after: "after hours",
    closed: "closed",
  };
  return t.state ? labels[t.state] : "";
}

/** `+0.8%`, `−1.2%` (a real minus, so the sign lines up with the plus). */
export function percent(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  if (rounded === 0) return "0.0%";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded).toFixed(1)}%`;
}

/** 232.14, 85,809.20: two decimals and grouping, whatever the magnitude. */
export function price(value: number): string {
  return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/** `9:30 AM`, in the user's own clock style. */
export function timeOfDay(ms: number, locale?: string): string {
  return new Date(ms).toLocaleTimeString(locale, { hour: "numeric", minute: "2-digit" });
}

const startOfDay = (ms: number) => new Date(ms).setHours(0, 0, 0, 0);

/**
 * Whether a reminder is late. A timed one is late once its time passes; an
 * all-day one only once its day has — due "today" at 00:00 is not overdue at
 * 9am.
 */
export function overdue(due: number, allDay: boolean, now: number): boolean {
  return allDay ? due < startOfDay(now) : due < now;
}

/** `Today`, `4:00 PM`, `Overdue · Mon 4:00 PM`: when a reminder is due, in words. */
export function dueText(due: number, allDay: boolean, now: number, locale?: string): string {
  const late = overdue(due, allDay, now);
  const sameDay = startOfDay(due) === startOfDay(now);
  const day = new Date(due).toLocaleDateString(locale, { weekday: "short" });
  const time = allDay ? "" : timeOfDay(due, locale);
  if (late) {
    const when = sameDay ? time : [day, time].filter(Boolean).join(" ");
    return `Overdue · ${when}`;
  }
  return allDay ? "Today" : time;
}

/** `now` while it is on, else `in 25 min`, rounding up to the minute like the island. */
export function untilText(start: number, end: number, now: number): string {
  if (start <= now && now < end) return "now";
  const mins = Math.max(1, Math.ceil((start - now) / 60_000));
  if (mins < 60) return `in ${mins} min`;
  const h = Math.floor(mins / 60);
  return mins % 60 === 0 ? `in ${h} hr` : `in ${h} hr ${mins % 60} min`;
}

/**
 * The event the island is showing, so the view can mark the same one: what
 * starts within the hour, else what is running. Mirrors `next_event` in
 * `crates/agenda`; all-day events are the day, not the next thing.
 */
export function nextEvent(events: CalendarEvent[], now: number): CalendarEvent | undefined {
  const timed = events.filter((e) => !e.allDay && e.end > now);
  const soon = timed
    .filter((e) => e.start > now && e.start - now <= 60 * 60_000)
    .sort((a, b) => a.start - b.start)[0];
  return soon ?? timed.filter((e) => e.start <= now).sort((a, b) => a.end - b.end)[0];
}
