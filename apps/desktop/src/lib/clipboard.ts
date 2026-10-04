/**
 * Time since a copy, the way a glance wants it: "now", "5m", "2h", "3d".
 * Seconds are not shown — nothing here is that precise — and a clock that
 * runs behind the entry (a future time) reads "now", never a negative.
 */
export function ago(thenSeconds: number, nowSeconds = Math.floor(Date.now() / 1000)): string {
  const s = Math.max(0, nowSeconds - thenSeconds);
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86_400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86_400)}d`;
}

/** `#rrggbb` as the `rgb(r, g, b)` CSS takes, or the input back if it is not hex. */
export function hexToRgb(hex: string): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  if (!m) return hex;
  return `rgb(${parseInt(m[1], 16)}, ${parseInt(m[2], 16)}, ${parseInt(m[3], 16)})`;
}

/** Snippets matching every word of `query` in their name, trigger or text. */
export function filterSnippets<T extends { name: string; trigger: string; body: string }>(
  snippets: T[],
  query: string,
): T[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return snippets;
  return snippets.filter((s) => {
    const hay = `${s.name} ${s.trigger} ${s.body}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
