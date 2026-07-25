import { describe, expect, it } from "vitest";
import {
  abbreviateCount,
  formatCost,
  formatElapsed,
  resetsIn,
  timeAgo,
  tokenSummary,
} from "./format";

describe("abbreviateCount", () => {
  it("rolls up through k, M, B", () => {
    expect(abbreviateCount(999)).toBe("999");
    expect(abbreviateCount(2000)).toBe("2k");
    expect(abbreviateCount(308393000)).toBe("308.4M"); // not "308393k"
    expect(abbreviateCount(2_600_000_000)).toBe("2.6B");
  });
  it("trims a trailing .0", () => {
    expect(abbreviateCount(2_000_000)).toBe("2M");
  });
});

describe("formatCost", () => {
  it("shows dollars and cents", () => {
    expect(formatCost(1.234)).toBe("$1.23");
    expect(formatCost(0.04)).toBe("$0.04");
  });
  it("rounds a sub-cent real cost up to $0.01, never $0.00", () => {
    expect(formatCost(0.004)).toBe("$0.01");
  });
});

describe("tokenSummary", () => {
  it("labels each part in words, biggest unit", () => {
    expect(tokenSummary(2000, 308393000, 542000)).toBe("2k in · 308.4M cache · 542k out");
    expect(tokenSummary(999, 0, 1000)).toBe("999 in · 1k out");
  });
  it("drops the cache part when cache-read is zero", () => {
    expect(tokenSummary(10, 0, 5)).toBe("10 in · 5 out");
  });
});

describe("resetsIn", () => {
  const now = Date.parse("2026-07-22T21:00:00.000Z");
  it("counts forward: minutes, hours, days", () => {
    expect(resetsIn("2026-07-22T21:05:00.000Z", now)).toBe("5m");
    expect(resetsIn("2026-07-22T23:00:00.000Z", now)).toBe("2h");
    expect(resetsIn("2026-07-25T21:00:00.000Z", now)).toBe("3d");
  });
  it("reads <1m when nearly due", () => {
    expect(resetsIn("2026-07-22T21:00:30.000Z", now)).toBe("<1m");
  });
  it("is empty on an unparseable stamp", () => {
    expect(resetsIn("nope", now)).toBe("");
  });
});

describe("timeAgo", () => {
  const now = Date.parse("2026-07-22T21:00:00.000Z");
  it("reads now, then minutes, then hours", () => {
    expect(timeAgo("2026-07-22T20:59:30.000Z", now)).toBe("now");
    expect(timeAgo("2026-07-22T20:50:00.000Z", now)).toBe("10m");
    expect(timeAgo("2026-07-22T19:00:00.000Z", now)).toBe("2h");
  });
  // A clock skew that puts the stamp slightly ahead must read "now", not a
  // negative age.
  it("does not go negative on a future stamp", () => {
    expect(timeAgo("2026-07-22T21:00:05.000Z", now)).toBe("now");
  });
  it("is empty on an unparseable stamp", () => {
    expect(timeAgo("not a date", now)).toBe("");
  });
});

describe("formatElapsed", () => {
  /** The failure this guards: dropping the pad turns 1:05 into 1:5, and
   *  dropping the hour branch turns a 90-minute meeting into "90:00". */
  it("pads and rolls over into hours", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(65)).toBe("1:05");
    expect(formatElapsed(3599)).toBe("59:59");
    expect(formatElapsed(3600)).toBe("1:00:00");
    expect(formatElapsed(5400)).toBe("1:30:00");
  });
});
