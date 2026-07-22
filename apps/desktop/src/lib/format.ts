/** `date` is unix seconds from Rust. Middot separator, as in the design. */
export function formatDate(date: number) {
  const at = new Date(date * 1000);
  return `${at.toLocaleDateString(undefined, { dateStyle: "medium" })} · ${at.toLocaleTimeString(undefined, { timeStyle: "short" })}`;
}

/**
 * A dictation's length. Sub-second runs are common enough that rounding them
 * all to "0s" would make the column useless.
 */
export function formatDuration(ms: number) {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/**
 * How long ago an ISO-8601 timestamp was, the way a menu-bar tag wants it:
 * `now` under a minute, then `10m`, then `2h`. Ported from agent-notch's
 * `relative()` so an agent row here reads the same as it did there.
 *
 * `now` is a parameter so the maths is testable without mocking the clock.
 */
export function timeAgo(iso: string, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor((now - Date.parse(iso)) / 1000));
  if (Number.isNaN(seconds)) return "";
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h`;
}

/**
 * A meeting's running length: `mm:ss` under an hour, `h:mm:ss` over it.
 * Meetings routinely run past both, unlike a dictation.
 */
export function formatElapsed(seconds: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}
