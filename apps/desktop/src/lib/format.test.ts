import { describe, expect, it } from "vitest";
import { formatElapsed } from "./format";

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
