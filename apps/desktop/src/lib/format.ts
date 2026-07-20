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
