import { describe, expect, it } from "vitest";
import { notchPath } from "./notch-shape";

/** Every coordinate pair in the path, in order. */
function points(d: string): Array<[number, number]> {
  const numbers = d.match(/-?\d+(\.\d+)?/g)?.map(Number) ?? [];
  const out: Array<[number, number]> = [];
  for (let i = 0; i + 1 < numbers.length; i += 2) out.push([numbers[i], numbers[i + 1]]);
  return out;
}

describe("notchPath", () => {
  it("spans the full width at the top edge, so it welds to the screen", () => {
    const d = notchPath(200, 80, 8, 16);
    // The concave shoulders exist precisely so the first and last points touch
    // both corners of the box. If they inset, the panel detaches from the notch.
    expect(d.startsWith("M 0 0")).toBe(true);
    expect(d).toContain("200 0");
  });

  it("insets the body by the top radius, which is what makes the corner concave", () => {
    const d = notchPath(200, 80, 8, 16);
    const xs = points(d).map(([x]) => x);
    expect(xs).toContain(8); // left wall, inset by topRadius
    expect(xs).toContain(192); // right wall
  });

  // The failure this guards: mid-spring the panel is briefly narrower than its
  // own corner radii, and an unclamped path folds through itself — a visible
  // black spike for a frame or two on every open.
  it("never lets the radii cross the midline when the panel is narrow", () => {
    const d = notchPath(10, 80, 34, 44);
    const xs = points(d).map(([x]) => x);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...xs)).toBeLessThanOrEqual(10);
    expect(d).not.toContain("NaN");
  });

  it("survives a zero-size panel without emitting NaN", () => {
    expect(notchPath(0, 0, 8, 16)).not.toContain("NaN");
  });

  it("clamps a bottom radius taller than the panel", () => {
    const ys = points(notchPath(200, 10, 8, 44)).map(([, y]) => y);
    expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...ys)).toBeLessThanOrEqual(10);
  });
});
