import { describe, expect, it } from "vitest";
import { createSpring, integrate } from "./spring";

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

/**
 * Runs a spring off a hand-cranked clock instead of a display.
 *
 * Frames are fed at a fixed 16ms so a test never depends on wall time; the
 * timestamps start from the real clock because `to()` reads `performance.now()`
 * when it starts, and a zero here would hand the first step a negative dt.
 */
function drive(
  build: (onFrame: (value: number) => void) => {
    spring: ReturnType<typeof createSpring>;
    values: number[];
  },
) {
  const pending: Array<(time: number) => void> = [];
  const realRequest = globalThis.requestAnimationFrame;
  const realCancel = globalThis.cancelAnimationFrame;
  globalThis.requestAnimationFrame = ((callback: (time: number) => void) => {
    pending.push(callback);
    return pending.length;
  }) as typeof globalThis.requestAnimationFrame;
  globalThis.cancelAnimationFrame = (() => {}) as typeof globalThis.cancelAnimationFrame;

  const values: number[] = [];
  const { spring } = build((value) => values.push(value));

  const start = performance.now();
  // Cumulative across calls: a per-call counter would hand the second `step`
  // an earlier timestamp than the first ended on, and the negative dt makes the
  // spring integrate backwards.
  let elapsed = 0;
  const step = (frames: number) => {
    for (let i = 0; i < frames; i++) {
      const next = pending.shift();
      if (!next) return;
      elapsed += 16;
      next(start + elapsed);
    }
  };

  const restore = () => {
    globalThis.requestAnimationFrame = realRequest;
    globalThis.cancelAnimationFrame = realCancel;
  };

  return { spring, values, step, restore };
}

describe("createSpring response retuning", () => {
  /**
   * The reason `to()` takes a response at all: the drop-down panel opens on one
   * and closes on a slower one, and picking between them must not mean throwing
   * the spring away and building a new one mid-motion.
   */
  it("makes a slower response actually travel more slowly", () => {
    const measure = (response?: number) => {
      const driver = drive((onFrame) => {
        const spring = createSpring(onFrame, { damping: 1, response: 0.3 });
        spring.jump(0);
        spring.to(100, response);
        return { spring, values: [] };
      });
      driver.step(12);
      const travelled = driver.values[driver.values.length - 1] ?? 0;
      driver.restore();
      return travelled;
    };

    const fast = measure(0.3);
    const slow = measure(0.9);

    expect(fast).toBeGreaterThan(slow);
  });

  it("keeps the motion already in flight when the response changes", () => {
    const driver = drive((onFrame) => {
      const spring = createSpring(onFrame, { damping: 1, response: 0.3 });
      spring.jump(0);
      spring.to(100);
      return { spring, values: [] };
    });

    driver.step(6);
    const midFlight = driver.values[driver.values.length - 1];
    expect(midFlight).toBeGreaterThan(0);

    // Retuning must not reset anything: the next frame has to continue from
    // where the spring already was, not restart from the last target.
    driver.spring.to(200, 0.6);
    driver.step(1);
    const afterRetune = driver.values[driver.values.length - 1];

    expect(afterRetune).toBeGreaterThan(midFlight);
    driver.restore();
  });
});
