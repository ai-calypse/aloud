import type { SeriesPoint } from "@/lib/bindings";

/**
 * A day's line as the backend sends it: a seeded walk from the open that ends
 * at `last`, over `upTo` of the session (1 is a closed day, less is one still
 * going), so each fixture draws the same every reload.
 */
export function sessionFixture(seed: number, open: number, last: number, upTo = 1, bars = 78): SeriesPoint[] {
  let a = seed;
  const rand = () => ((a = (a * 1664525 + 1013904223) >>> 0) / 2 ** 32) - 0.5;
  const n = Math.max(2, Math.round(bars * upTo));
  // A random walk pinned at both ends (a bridge), so it wanders but arrives where it must.
  const walk: number[] = [0];
  for (let i = 1; i < n; i++) walk.push(walk[i - 1] + rand());
  const amp = (Math.abs(last - open) + open * 0.004) * 0.35;
  return Array.from({ length: n }, (_, i) => {
    const t = i / (n - 1);
    const bridge = walk[i] - walk[n - 1] * t;
    return { x: i / (bars - 1), y: i === n - 1 ? last : open + (last - open) * t + bridge * amp };
  });
}
