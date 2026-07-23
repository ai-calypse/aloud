import { describe, expect, it } from "vitest";
import { formatElapsed, timeAgo } from "./format";

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
