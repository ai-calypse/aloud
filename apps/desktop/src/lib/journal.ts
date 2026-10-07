import type { Note } from "./bindings";

/** One day of the journal's timeline, newest entries first. */
export type JournalDay = { key: string; label: string; entries: Note[] };

const DAY_MS = 86_400_000;

/** Local midnight of the day `at` falls on. */
function midnight(at: Date): Date {
  return new Date(at.getFullYear(), at.getMonth(), at.getDate());
}

/**
 * Whole calendar days from `at` to `now`, in local time. Rounded rather than
 * floored: a day that holds a daylight-saving change is 23 or 25 hours long,
 * and flooring would call yesterday "today" once a year.
 */
export function daysBefore(at: Date, now: Date): number {
  return Math.round((midnight(now).getTime() - midnight(at).getTime()) / DAY_MS);
}

/** What a day is called above its entries: the way a person says it. */
export function dayLabel(at: Date, now: Date): string {
  const days = daysBefore(at, now);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return at.toLocaleDateString(undefined, { weekday: "long" });
  return at.toLocaleDateString(
    undefined,
    at.getFullYear() === now.getFullYear()
      ? { month: "long", day: "numeric" }
      : { month: "long", day: "numeric", year: "numeric" },
  );
}

/**
 * Entries grouped by the local day they were written, keeping their order —
 * Rust already sorts them newest first. `date` is unix seconds.
 */
export function groupByDay(entries: Note[], now = new Date()): JournalDay[] {
  const days: JournalDay[] = [];
  for (const entry of entries) {
    const at = new Date(entry.date * 1000);
    const key = `${at.getFullYear()}-${at.getMonth()}-${at.getDate()}`;
    const last = days[days.length - 1];
    if (last?.key === key) last.entries.push(entry);
    else days.push({ key, label: dayLabel(at, now), entries: [entry] });
  }
  return days;
}

/**
 * The entry after the line that names it, on one line, for the list's preview.
 * Empty when the whole entry is that one line — the row then shows the line
 * alone rather than repeating it.
 */
export function entryPreview(content: string): string {
  const lines = content.split("\n");
  const first = lines.findIndex((l) => l.trim().replace(/^#+/, "").trim() !== "");
  if (first === -1) return "";
  return lines
    .slice(first + 1)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Spoken or typed, a thought is short: the count the composer shows. */
export function wordCount(text: string): number {
  const trimmed = text.trim();
  return trimmed === "" ? 0 : trimmed.split(/\s+/).length;
}
