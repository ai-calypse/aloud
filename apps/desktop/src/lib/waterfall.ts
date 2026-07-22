/**
 * The arithmetic behind the timings waterfall. Out of the component because
 * both functions can be quietly wrong — a regression looks like timings that
 * are fine, so they are tested rather than eyeballed.
 */

import type { Span } from "./bindings";

/**
 * A span's recorded fields. Free-form JSON, so a parse failure is plausible and
 * must not take the pane down.
 */
export function spanFields(span: Span): Record<string, unknown> {
  try {
    return JSON.parse(span.fields);
  } catch {
    return {};
  }
}

/**
 * Wall-clock the stages actually cover. Spans nest — `llm_fallback` runs inside
 * `cleanup` — so summing durations double-counts and drove the unaccounted
 * remainder negative. This is an interval union.
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
 * Nearest-rank percentile. An off-by-one here is invisible: p95 quietly reports
 * p90 and the number still looks reasonable.
 */
export function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(Math.max(rank, 0), sorted.length - 1)];
}
