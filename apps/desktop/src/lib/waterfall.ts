/**
 * The arithmetic behind the timings waterfall.
 *
 * Out of the component because both functions can be quietly wrong: they
 * produce a plausible number either way, so a regression shows up as timings
 * that look fine and are not. Neither throws, neither fails a typecheck, and
 * nothing on screen says "this is off by one".
 */

import type { Span } from "./bindings";

/**
 * A span's recorded fields. Free-form JSON written by whatever the call site
 * chose to record, so a parse failure is a plausible outcome and must not take
 * the pane down — both the waterfall and the Run Log render around it.
 */
export function spanFields(span: Span): Record<string, unknown> {
  try {
    return JSON.parse(span.fields);
  } catch {
    return {};
  }
}

/**
 * Wall-clock the stages actually cover.
 *
 * Spans nest — `llm_fallback` runs inside `cleanup` — so summing durations
 * double-counts, which drove the unaccounted remainder negative. This is an
 * interval union: sort by start, and only count the part of each span that
 * reaches past where the previous one ended.
 */
export function covered(stages: Span[]): number {
  const spans = [...stages].sort((a, b) => a.startedAt - b.startedAt);
  let total = 0;
  let end = -Infinity;
  for (const s of spans) {
    const start = Math.max(s.startedAt, end);
    end = Math.max(end, s.startedAt + s.durationMs);
    total += Math.max(end - start, 0);
  }
  return total;
}

/**
 * Nearest-rank percentile. Averages hide the tail; this is what to read.
 *
 * An off-by-one here is invisible: p95 quietly reports p90 and the number
 * still looks reasonable.
 */
export function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(Math.max(rank, 0), sorted.length - 1)];
}
