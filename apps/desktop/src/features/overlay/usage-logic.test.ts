import { describe, expect, it } from "vitest";
import type { Quota, QuotaSample } from "@/lib/bindings";
import { chartLine, countdown, elapsedFraction, extraSpend, pace, shownWindows } from "./usage-logic";

const NOW = Date.parse("2026-07-24T12:00:00Z");
const HOUR = 3_600_000;
const sample = (hoursAgo: number, fiveHour: number | null, resets: number | null = null): QuotaSample => ({
  at: (NOW - hoursAgo * HOUR) / 1000,
  fiveHour,
  fiveHourResetsAt: resets,
  sevenDay: null,
  sevenDayResetsAt: null,
});
const five = (s: QuotaSample) => ({ value: s.fiveHour, resetsAt: s.fiveHourResetsAt });

describe("shownWindows", () => {
  it("lists every window the plan reports and none it left out", () => {
    const quota = {
      fiveHour: { percent: 10, resetsAt: null },
      sevenDay: { percent: 20, resetsAt: null },
      sevenDayOpus: null,
      sevenDaySonnet: { percent: 3, resetsAt: null },
      sevenDayCowork: { percent: 1, resetsAt: null },
    } as Quota;
    expect(shownWindows(quota).map((w) => w.label)).toEqual([
      "5-hour window",
      "Week · all models",
      "Week · Sonnet",
      "Week · Routines",
    ]);
    expect(shownWindows(null)).toEqual([]);
  });
});

describe("countdown", () => {
  it("keeps the second unit", () => {
    const at = (ms: number) => new Date(NOW + ms).toISOString();
    expect(countdown(at(14 * 60_000), NOW)).toBe("14 m");
    expect(countdown(at(2 * HOUR + 14 * 60_000), NOW)).toBe("2 h 14 m");
    expect(countdown(at(3 * 24 * HOUR + 4 * HOUR), NOW)).toBe("3 d 4 h");
    expect(countdown(at(10_000), NOW)).toBe("<1 m");
  });
});

describe("pace", () => {
  it("compares use to how much of the window has gone", () => {
    expect(pace(60, 0.25)).toBe("Ahead of pace");
    expect(pace(10, 0.6)).toBe("Under pace");
    expect(pace(52, 0.5)).toBe("On pace");
  });

  /** Guards: a window a minute old calling 3% "ahead of pace". */
  it("has no verdict for a young window, and calls a full one reached", () => {
    expect(pace(3, 0.01)).toBeNull();
    expect(pace(100, 0.2)).toBe("Limit reached");
    expect(pace(40, null)).toBeNull();
  });

  it("reads elapsed time off the reset", () => {
    expect(elapsedFraction(new Date(NOW + 2 * HOUR).toISOString(), 5 * HOUR, NOW)).toBeCloseTo(0.6);
    expect(elapsedFraction(null, 5 * HOUR, NOW)).toBeNull();
    expect(elapsedFraction("not a date", 5 * HOUR, NOW)).toBeNull();
  });
});

describe("extraSpend", () => {
  it("turns minor units into money", () => {
    expect(extraSpend({ enabled: true, monthlyLimit: 5000, usedCredits: 479, currency: "USD", utilization: 9.58 })).toEqual({
      spent: "$4.79",
      limit: "$50.00",
      fraction: 479 / 5000,
    });
  });

  it("says nothing when the pool has no cap to measure against", () => {
    expect(extraSpend({ enabled: false, monthlyLimit: null, usedCredits: null, currency: null, utilization: null })).toBeNull();
  });
});

describe("chartLine", () => {
  const W = 120;
  const H = 40;

  it("puts now at the right edge and 100% at the top", () => {
    const line = chartLine([sample(0.05, 0), sample(0, 100)], five, HOUR, NOW, W, H);
    expect(line.segments).toEqual([[[114, 40], [120, 0]]]);
    expect(line.last).toEqual([120, 0]);
  });

  /** Guards: a long silence between readings drawn as a slope that never happened. */
  it("breaks the line where readings are far apart and the reset is unknown", () => {
    const line = chartLine([sample(20, 10), sample(19.8, 12), sample(2, 40), sample(1.8, 42)], five, 24 * HOUR, NOW, W, H);
    expect(line.segments.map((s) => s.length)).toEqual([2, 2]);
  });

  /** Guards: bursts of readings drawing as unconnected sticks, and a reset drawn as a slope. */
  it("joins readings of one window across a gap, and breaks at a reset", () => {
    const resetA = Math.floor(NOW / 1000) - 3600;
    const resetB = resetA + 5 * 3600;
    const line = chartLine(
      [sample(9, 10, resetA), sample(5, 40, resetA + 20), sample(2, 15, resetB), sample(0, 30, resetB)],
      five,
      24 * HOUR,
      NOW,
      W,
      H,
    );
    expect(line.segments.map((s) => s.length)).toEqual([2, 2]);
  });

  it("leaves out readings older than the span and windows the plan did not report", () => {
    const line = chartLine([sample(30, 50), sample(5, null), sample(1, 20)], five, 24 * HOUR, NOW, W, H);
    expect(line.points).toHaveLength(1);
    expect(chartLine([], five, 24 * HOUR, NOW, W, H).last).toBeNull();
  });
});
