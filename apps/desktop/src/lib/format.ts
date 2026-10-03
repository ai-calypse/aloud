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

/** `$1.23` — a session cost estimate. Sub-cent rounds up to `$0.01`, so a real
 *  (if tiny) cost never displays as free. */
export function formatCost(usd: number): string {
  return `$${Math.max(usd, 0.005).toFixed(2)}`;
}

/** A token count abbreviated with the right unit: `999`, `2k`, `308M`, `2.6B`.
 *  The old `k`-only form rendered 308 million as an unreadable `308393k`. */
export function abbreviateCount(n: number): string {
  if (n >= 1e9) return `${round1(n / 1e9)}B`;
  if (n >= 1e6) return `${round1(n / 1e6)}M`;
  if (n >= 1e3) return `${Math.round(n / 1e3)}k`;
  return `${n}`;
}

/** Trim a trailing `.0` so `2.0M` reads as `2M`. */
function round1(n: number): string {
  return (Math.round(n * 10) / 10).toString();
}

/**
 * `2k in · 308M cache · 542k out` — input, cache-read, output token totals,
 * labelled in words instead of arrows so the line reads without a legend.
 * Cache-read is kept separate (a distinct, cheaper cost) and dropped entirely
 * when zero.
 */
export function tokenSummary(input: number, cacheRead: number, output: number): string {
  const parts = [`${abbreviateCount(input)} in`];
  if (cacheRead > 0) parts.push(`${abbreviateCount(cacheRead)} cache`);
  parts.push(`${abbreviateCount(output)} out`);
  return parts.join(" · ");
}

/**
 * How long until a future ISO-8601 timestamp, for a "resets in …" label:
 * `<1m`, then `5m`, then `2h`, then `3d`. The mirror of {@link timeAgo}, facing
 * forward. `now` is a parameter so the maths is testable.
 */
export function resetsIn(iso: string, now = Date.now()): string {
  const seconds = Math.floor((Date.parse(iso) - now) / 1000);
  if (Number.isNaN(seconds)) return "";
  if (seconds < 60) return "<1m";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
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

/** Cuts on a char boundary so a multi-byte character is never split. */
export function truncate(text: string, max: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return [...flat].length <= max ? flat : `${[...flat].slice(0, max).join("")}…`;
}
