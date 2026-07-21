/**
 * A spring you can redirect mid-flight without it losing its momentum.
 *
 * Almost all motion in this app is CSS, and should stay CSS — the compositor
 * owns those frames, so they do not stutter while React renders (see
 * styles/motion.css). This module exists for the one thing CSS cannot express.
 *
 * A CSS transition interrupted halfway *does* retarget from where it is, but it
 * restarts its easing curve from zero speed. Click one sidebar row and then
 * another before the first move lands and you see it: the chip is travelling,
 * then stalls for a frame, then sets off again. A real object does not do that.
 * A spring carries its velocity through a change of target, because the target
 * is just a term in the acceleration — nothing about a retarget resets the
 * state. That property is the entire reason this file is not a CSS transition.
 *
 * Parameters are Apple's two, matching the tokens in motion.css: a damping
 * ratio (1 = settles without overshoot) and a response in seconds (how quickly
 * it reaches the target — not a duration; a spring has no fixed duration).
 */

/** Below this distance and speed the motion is not visible; stop integrating. */
const REST = 0.01;

/**
 * One step of a damped harmonic oscillator, in isolation so it can be tested
 * without a display.
 *
 * Semi-implicit Euler: velocity is updated first and the *new* velocity moves
 * the position. Explicit Euler (position first) feeds energy into the system
 * and a stiff spring visibly gains amplitude instead of settling.
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
 * Integration step, in seconds.
 *
 * A frame is ~16ms, but integrating a stiff spring in one 16ms jump overshoots
 * badly enough to oscillate — the error grows with omega·dt. Sub-stepping at
 * 240Hz keeps it stable at any response we would plausibly use, and costs a
 * handful of multiplications per frame.
 */
const SUB_STEP = 1 / 240;

export type Spring = {
  /**
   * Animate towards `value`, keeping whatever velocity is already in flight.
   *
   * `response` retunes the spring for this leg and every leg after it, without
   * disturbing the motion already underway.
   */
  to(value: number, response?: number): void;
  /** Jump to `value` with no motion and no residual velocity. */
  jump(value: number): void;
  /** Cancel any frame in flight. Call from the effect's cleanup. */
  stop(): void;
};

/**
 * Drives `onFrame` with a spring towards whatever target is current.
 *
 * `onFrame` is called on the animation frame, so it should write a `transform`
 * straight onto a node — not set React state, which would put a render between
 * every frame and defeat the point.
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
    // A window that was hidden (App Nap suspends this webview) returns with a
    // multi-second gap. Clamping keeps that from teleporting the element.
    //
    // Floored at zero as well, because a negative dt does not merely stall the
    // spring — it integrates it *backwards*, away from the target. An animation
    // frame carries the time the frame began, which can be marginally earlier
    // than a `performance.now()` read moments before it in `start`, so the very
    // first step of any motion is the one exposed to this.
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
      // Retuning on the way to a new target, not on the way to the same one:
      // opening and closing are allowed to have different characters (a close
      // that lingers slightly longer than the open is what reads as weight),
      // and the spring keeps its velocity across the change because only omega
      // moves — position and velocity are untouched.
      if (retargetResponse !== undefined) {
        omega = (2 * Math.PI) / retargetResponse;
      }
      if (value === target) return;
      target = value;
      // Reduced motion is not "a faster spring" — it is no travel at all. The
      // OS setting is the user saying movement makes them unwell.
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
