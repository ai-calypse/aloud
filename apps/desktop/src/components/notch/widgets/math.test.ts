import { describe, expect, it } from "vitest";
import {
  dashOffset,
  formatNumeral,
  litSegments,
  numeralParts,
  ringGeometry,
  sparkPath,
  stackGeometry,
  unit,
} from "./math";

describe("ring math", () => {
  it("clamps whatever arrives into 0…1, and draws NaN empty rather than full", () => {
    expect([unit(-3), unit(0.4), unit(9), unit(NaN), unit(Infinity)]).toEqual([0, 0.4, 1, 0, 0]);
  });

  it("leaves nothing undrawn at 1 and everything at 0", () => {
    const { circumference } = ringGeometry(100, 10);
    expect(dashOffset(circumference, 1)).toBe(0);
    expect(dashOffset(circumference, 0)).toBe(circumference);
    expect(dashOffset(circumference, 0.25)).toBeCloseTo(circumference * 0.75);
  });

  it("centres the stroke inside the box, so it is never clipped", () => {
    expect(ringGeometry(100, 10).r).toBe(45);
    expect(ringGeometry(4, 10).r).toBe(0);
  });
});

describe("stackGeometry", () => {
  const rings = stackGeometry(52, 3);

  it("nests the rings with no overlap, outermost first", () => {
    expect(rings).toHaveLength(3);
    for (let i = 1; i < rings.length; i++) {
      // The outer edge of the inner ring is inside the inner edge of the outer one.
      const outerInnerEdge = rings[i - 1].r - rings[i - 1].stroke / 2;
      const innerOuterEdge = rings[i].r + rings[i].stroke / 2;
      expect(innerOuterEdge).toBeLessThan(outerInnerEdge);
    }
  });

  it("fits the box and leaves a hole in the middle", () => {
    expect(rings[0].r + rings[0].stroke / 2).toBeCloseTo(26);
    const last = rings[2];
    expect(last.r - last.stroke / 2).toBeGreaterThan(0);
  });
});

describe("litSegments", () => {
  it("rounds to the nearest and never leaves the bar", () => {
    expect(litSegments(0.5, 10)).toBe(5);
    expect(litSegments(0.26, 10)).toBe(3);
    expect(litSegments(2, 10)).toBe(10);
    expect(litSegments(NaN, 10)).toBe(0);
  });
});

describe("formatNumeral", () => {
  it("rounds to the decimals asked for", () => {
    expect(formatNumeral(82)).toBe("82");
    expect(formatNumeral(2.456, { decimals: 1 })).toBe("2.5");
  });

  it("groups only when asked, in ASCII", () => {
    expect(formatNumeral(1234567)).toBe("1234567");
    expect(formatNumeral(1234567, { group: true })).toBe("1,234,567");
  });

  it("shows a dash for no value rather than NaN", () => {
    expect([formatNumeral(null), formatNumeral(undefined), formatNumeral(NaN)]).toEqual(["–", "–", "–"]);
  });

  it("never shows -0, and uses a real minus otherwise", () => {
    expect(formatNumeral(-0.2)).toBe("0");
    expect(formatNumeral(-3)).toBe("−3");
  });
});

describe("numeralParts", () => {
  it("splits digits from the characters between them", () => {
    expect(numeralParts("1:05")).toEqual([
      { kind: "digit", digit: 1 },
      { kind: "char", char: ":" },
      { kind: "digit", digit: 0 },
      { kind: "digit", digit: 5 },
    ]);
  });
});

describe("sparkPath", () => {
  it("draws nothing for fewer than two points", () => {
    expect(sparkPath([], 10, 10)).toBe("");
    expect(sparkPath([3], 10, 10)).toBe("");
  });

  it("spans the width, with the high point at the top", () => {
    const path = sparkPath([0, 10], 100, 20, 0);
    expect(path).toBe("M0.0 20.0 L100.0 0.0");
  });

  it("draws a flat series mid-height instead of dividing by zero", () => {
    expect(sparkPath([5, 5, 5], 10, 20)).toBe("M0.0 10.0 L5.0 10.0 L10.0 10.0");
  });
});
