/**
 * A spring you can redirect mid-flight without it losing its momentum.
 *
 * Almost all motion here is CSS and should stay CSS (styles/motion.css). This
 * exists for the one thing CSS cannot express: a transition interrupted halfway
 * retargets from where it is, but restarts its curve from zero speed — the chip
 * travels, stalls for a frame, sets off again. A spring carries its velocity
 * through a retarget, because the target is only a term in the acceleration.
 *
 * Parameters are Apple's two, matching motion.css: a damping ratio (1 = settles
 * without overshoot) and a response in seconds — not a duration.
 */

/** Below this distance and speed the motion is not visible; stop integrating. */
const REST = 0.01;

/**
 * One step of a damped harmonic oscillator, in isolation so it can be tested
 * without a display. Semi-implicit Euler: velocity first, and the *new*
 * velocity moves the position — explicit Euler feeds energy in, and a stiff
 * spring visibly gains amplitude instead of settling.
 */
export function integrate(
  position: number,
  velocity: number,
  target: number,
  /** Seconds. Must be small — see `SUB_STEP`. */
  dt: number,
  damping: number,
  /** Natural frequency, rad/s: 2π / response. */
  omega: number,
): { position: number; velocity: number } {
  const acceleration =
    -omega * omega * (position - target) - 2 * damping * omega * velocity;
  const nextVelocity = velocity + acceleration * dt;
  return {
    position: position + nextVelocity * dt,
    velocity: nextVelocity,
  };
}

/**
 * Integration step, in seconds. One 16ms jump overshoots a stiff spring badly
 * enough to oscillate (the error grows with omega·dt); sub-stepping at 240Hz is
 * stable at any response we would plausibly use.
 */
const SUB_STEP = 1 / 240;

export type Spring = {
  /**
   * Animate towards `value`, keeping whatever velocity is already in flight.
   * `response` retunes the spring for this leg and every leg after it.
   */
  to(value: number, response?: number): void;
  /** Jump to `value` with no motion and no residual velocity. */
  jump(value: number): void;
  /** Cancel any frame in flight. Call from the effect's cleanup. */
  stop(): void;
};

/**
 * Drives `onFrame` with a spring towards whatever target is current. `onFrame`
 * runs on the animation frame, so it should write straight onto a node — React
 * state would put a render between every frame.
 */
export function createSpring(
  onFrame: (value: number) => void,
  {
    damping = 1,
    response = 0.3,
    reducedMotion = typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches,
  }: { damping?: number; response?: number; reducedMotion?: boolean } = {},
): Spring {
  let omega = (2 * Math.PI) / response;

  let position = 0;
  let velocity = 0;
  let target = 0;
  let frame = 0;
  let previous = 0;

  const step = (now: number) => {
    // App Nap returns a hidden window with a multi-second gap; clamping stops
    // that teleporting the element. Floored at zero too: a frame carries the
    // time it began, which can precede the `performance.now()` in `start`, and
    // a negative dt integrates the spring *backwards*.
    const dt = Math.max(0, Math.min((now - previous) / 1000, 1 / 30));
    previous = now;

    const steps = Math.max(1, Math.ceil(dt / SUB_STEP));
    const h = dt / steps;
    for (let i = 0; i < steps; i++) {
      ({ position, velocity } = integrate(
        position,
        velocity,
        target,
        h,
        damping,
        omega,
      ));
    }

    if (Math.abs(target - position) < REST && Math.abs(velocity) < REST) {
      position = target;
      velocity = 0;
      frame = 0;
      onFrame(position);
      return;
    }

    onFrame(position);
    frame = requestAnimationFrame(step);
  };

  const start = () => {
    if (frame) return;
    previous = performance.now();
    frame = requestAnimationFrame(step);
  };

  return {
    to(value, retargetResponse) {
      // Opening and closing may have different characters (a close that
      // lingers reads as weight). Only omega moves, so the velocity carries.
      if (retargetResponse !== undefined) {
        omega = (2 * Math.PI) / retargetResponse;
      }
      if (value === target) return;
      target = value;
      // Reduced motion is not "a faster spring" — it is no travel at all.
      if (reducedMotion) {
        this.jump(value);
        return;
      }
      start();
    },
    jump(value) {
      this.stop();
      position = target = value;
      velocity = 0;
      onFrame(position);
    },
    stop() {
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
    },
  };
}
