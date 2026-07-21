import { describe, expect, it } from "vitest";
import { overshoot, springCurve, springValue } from "./spring-curve";

/**
 * The tokens in motion.css, as shipped. This solver has to reproduce them
 * exactly, because the whole point of it is to *replace* the way they were
 * produced — a generator that disagreed with the values already on screen
 * would silently change the feel of the app the first time anything was
 * retuned, and the diff would look like a retune rather than a regression.
 */
const SPRING =
  "linear(0, 0.0438, 0.1419, 0.2604, 0.3801, 0.4909, 0.5883, 0.6712, 0.74, 0.7961, 0.8412, 0.8771, 0.9053, 0.9274, 0.9445, 0.9578, 0.968, 0.9758, 0.9817, 0.9862, 0.9896, 0.9922, 0.9942, 0.9956, 0.9967, 0.9976, 0.9982, 0.9987, 1)";

const SPRING_POP =
  "linear(0, 0.0575, 0.1948, 0.3681, 0.5455, 0.7068, 0.8404, 0.9423, 1.0131, 1.0566, 1.0782, 1.0835, 1.0779, 1.0661, 1.0515, 1.0367, 1.0234, 1.0125, 1.0042, 0.9985, 0.995, 0.9934, 0.993, 0.9936, 0.9946, 0.9958, 0.997, 0.9981, 1)";

describe("spring-curve", () => {
  it("regenerates the critically damped token", () => {
    const { curve, durationMs } = springCurve(1, 0.3);
    expect(curve).toBe(SPRING);
    expect(durationMs).toBe(441);
  });

  it("regenerates the overshooting token", () => {
    const { curve, durationMs } = springCurve(0.62, 0.4);
    expect(curve).toBe(SPRING_POP);
    expect(durationMs).toBe(654);
  });

  /** One curve, two durations — see the note on --spring-dur-slow. */
  it("scales the settle with the response and leaves the shape alone", () => {
    expect(springCurve(1, 0.4).durationMs).toBe(588);
    expect(springCurve(1, 0.4).curve).toBe(springCurve(1, 0.3).curve);
  });

  it("reports the overshoot the tokens document", () => {
    expect(overshoot(0.62)).toBeCloseTo(0.084, 3);
    expect(overshoot(1)).toBe(0);
  });

  /**
   * The failure that would not show up in a curve comparison: a spring that
   * starts where it already is has nothing to do, and a solver that returns a
   * non-zero first sample makes every animation in the app jump on frame one.
   */
  it("starts at rest", () => {
    expect(springValue(0, 0.62, 0.4)).toBe(0);
    expect(springValue(-1, 1, 0.3)).toBe(0);
  });
});
