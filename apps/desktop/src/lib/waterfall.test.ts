import { describe, expect, it } from "vitest";
import { covered, percentile } from "./waterfall";
import type { Span } from "./bindings";

/** Only the three fields `covered` reads; the rest of Span is irrelevant here. */
function span(startedAt: number, durationMs: number): Span {
  return { startedAt, durationMs } as Span;
}

describe("covered", () => {
  it("sums spans that do not touch", () => {
    expect(covered([span(0, 100), span(200, 50)])).toBe(150);
  });

  /** The bug this function exists for: llm_fallback runs inside cleanup. */
  it("counts a fully nested span once, not twice", () => {
    const cleanup = span(0, 500);
    const fallback = span(100, 200);
    expect(covered([cleanup, fallback])).toBe(500);
  });

  it("counts an overlap once", () => {
    expect(covered([span(0, 100), span(50, 100)])).toBe(150);
  });

  it("does not depend on input order", () => {
    const stages = [span(200, 50), span(0, 100), span(50, 100)];
    expect(covered(stages)).toBe(covered([...stages].reverse()));
  });

  it("handles the empty and single cases", () => {
    expect(covered([])).toBe(0);
    expect(covered([span(10, 40)])).toBe(40);
  });

  /**
   * The remainder is `total - covered`, so covered must never exceed the
   * utterance it belongs to — that is what drove it negative before.
   */
  it("never exceeds the span that encloses everything", () => {
    const total = 1000;
    const stages = [span(0, 400), span(100, 200), span(380, 300), span(900, 100)];
    expect(covered(stages)).toBeLessThanOrEqual(total);
  });
});

describe("percentile", () => {
  it("takes the nearest rank, not an interpolation", () => {
    const values = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    expect(percentile(values, 50)).toBe(5);
    expect(percentile(values, 95)).toBe(10);
    expect(percentile(values, 100)).toBe(10);
  });

  /** p95 silently reporting p90 is the failure this pins down. */
  it("distinguishes p50 from p95 on a skewed set", () => {
    const values = [10, 10, 10, 10, 10, 10, 10, 10, 10, 900];
    expect(percentile(values, 50)).toBe(10);
    expect(percentile(values, 95)).toBe(900);
  });

  it("sorts before ranking", () => {
    expect(percentile([9, 1, 5], 50)).toBe(5);
  });

  it("clamps rather than returning undefined at the edges", () => {
    expect(percentile([], 95)).toBe(0);
    expect(percentile([42], 95)).toBe(42);
    expect(percentile([1, 2, 3], 0)).toBe(1);
  });
});
