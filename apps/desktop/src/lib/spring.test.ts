import { describe, expect, it } from "vitest";
import { integrate } from "./spring";

const DT = 1 / 240;

/** Integrates `steps` frames towards `target`, returning the final state. */
function run(
  position: number,
  velocity: number,
  target: number,
  steps: number,
  { damping = 1, response = 0.3 } = {},
) {
  const omega = (2 * Math.PI) / response;
  let peak = position;
  for (let i = 0; i < steps; i++) {
    ({ position, velocity } = integrate(
      position,
      velocity,
      target,
      DT,
      damping,
      omega,
    ));
    peak = Math.max(peak, position);
  }
  return { position, velocity, peak };
}

describe("spring", () => {
  it("settles on the target", () => {
    const { position, velocity } = run(0, 0, 100, 240); // 1s, response 0.3
    expect(position).toBeCloseTo(100, 1);
    expect(velocity).toBeCloseTo(0, 1);
  });

  it("does not overshoot when critically damped", () => {
    // The whole reason damping 1.0 is the default: a settings row chip that
    // sails past the row and comes back reads as a bug, not as physics.
    const { peak } = run(0, 0, 100, 480);
    expect(peak).toBeLessThanOrEqual(100.001);
  });

  it("overshoots when underdamped", () => {
    const { peak } = run(0, 0, 100, 480, { damping: 0.62, response: 0.4 });
    expect(peak).toBeGreaterThan(105);
    expect(peak).toBeLessThan(112); // a settle, not a bounce
  });

  /**
   * The regression this module exists for.
   *
   * Interrupting a CSS transition retargets from the current position but at
   * zero speed, so the element visibly stalls before setting off again. A
   * spring keeps the velocity it already had, so the redirected move continues
   * from the motion in progress — it must therefore cover more ground than one
   * starting from a standstill at the same place.
   */
  it("carries velocity through a retarget instead of restarting", () => {
    const midFlight = run(0, 0, 100, 60); // a quarter second in, still moving
    expect(midFlight.velocity).toBeGreaterThan(0);

    const carried = run(midFlight.position, midFlight.velocity, 200, 24);
    const restarted = run(midFlight.position, 0, 200, 24);

    expect(carried.position).toBeGreaterThan(restarted.position);
  });

  it("stays stable at the sub-step size for a stiff spring", () => {
    // Integrating a stiff spring in one 16ms frame gains energy and oscillates
    // instead of settling; SUB_STEP is what keeps this bounded.
    const { position, peak } = run(0, 0, 100, 720, { response: 0.15 });
    expect(peak).toBeLessThan(101);
    expect(position).toBeCloseTo(100, 1);
  });
});
