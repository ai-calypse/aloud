/**
 * How well `query` matches `text`, or null for no match. Ranked in tiers so a
 * better kind of match always beats a better-scored worse kind: a prefix, then
 * a word's start, then the query inside a word, then its letters in order
 * ("vsc" finds Visual Studio Code, "sfr" finds Safari). Case-insensitive.
 */
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().trim();
  const t = text.toLowerCase();
  if (!q) return 0;

  if (t.startsWith(q)) return 4000 - (t.length - q.length);
  const words = t.split(/[\s\-_.]+/).filter(Boolean);
  if (words.some((w, i) => i > 0 && w.startsWith(q))) return 3000 - t.indexOf(q);
  const at = t.indexOf(q);
  if (at >= 0) return 2000 - at;

  // Letters in order. Landing on a word's first letter is worth more than
  // landing mid-word, and a tight match more than a scattered one.
  let from = 0;
  let first = -1;
  let last = -1;
  let starts = 0;
  for (const ch of q) {
    const i = t.indexOf(ch, from);
    if (i < 0) return null;
    if (first < 0) first = i;
    if (i === 0 || /[\s\-_.]/.test(t[i - 1])) starts++;
    last = i;
    from = i + 1;
  }
  return 1000 + starts * 40 - (last - first);
}

/**
 * `items` that match `query`, best first; ties keep the order given. An empty
 * query keeps everything in order.
 */
export function rank<T>(items: T[], query: string, key: (item: T) => string): T[] {
  if (!query.trim()) return items;
  return items
    .map((item, index) => ({ item, index, score: fuzzyScore(query, key(item)) }))
    .filter((hit): hit is { item: T; index: number; score: number } => hit.score !== null)
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((hit) => hit.item);
}
