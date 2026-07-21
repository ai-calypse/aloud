/**
 * The spring solution behind the `linear()` tokens in motion.css.
 *
 * Those tokens are real springs sampled into a fixed curve, and until now the
 * sampling lived wherever they were first generated — so changing the feel of
 * the app meant hand-editing 29 numbers, or trusting a number someone typed.
 * This is the same oscillator `lib/spring.ts` integrates, solved analytically
 * instead of stepped, so a curve and the live spring beside it are the same
 * motion. `spring-curve.test.ts` pins that by regenerating the shipped tokens.
 *
 * Parameters are Apple's two, as everywhere else here: a damping ratio (1 =
 * settles without overshoot, below 1 = overshoots on arrival) and a response
 * in seconds.
 */

/** Below this the motion is over; the tail moves sub-pixel distances. */
const REST = 0.001;

/** Sample count in the shipped tokens. 29 puts a point every ~15ms at 0.3s. */
const SAMPLES = 29;

/**
 * Where a spring released from 0 towards 1 is at `t` seconds.
 *
 * The normalised shape depends only on the damping ratio — response merely
 * scales time — which is why one `linear()` serves every duration built from
 * the same ratio.
 */
export function springValue(t: number, damping: number, response: number): number {
  const omega = (2 * Math.PI) / response;
  if (t <= 0) return 0;
  // Critically damped is the limit case, not a special feel: the underdamped
  // form divides by √(1-ζ²) and blows up as ζ approaches 1.
  if (damping >= 0.999) return 1 - (1 + omega * t) * Math.exp(-omega * t);

  const decay = Math.exp(-damping * omega * t);
  const rest = Math.sqrt(1 - damping * damping);
  const damped = omega * rest;
  return (
    1 -
    decay * (Math.cos(damped * t) + (damping / rest) * Math.sin(damped * t))
  );
}

/**
 * How long a spring of this damping takes to settle, as a multiple of its
 * response.
 *
 * A multiple rather than a duration because settle time scales linearly with
 * response — which is what lets `--spring` be one curve run at two durations.
 *
 * Walked backwards rather than solved: an underdamped spring crosses the rest
 * threshold several times on the way down, once per half cycle, and only the
 * last crossing is the answer. Coarse pass to find which half cycle it is in,
 * then bisect inside it — a scan fine enough to place the crossing directly
 * would be ~10⁶ steps, and this runs on every drag of a tuning slider.
 */
export function settleRatio(damping: number): number {
  const step = 1e-4;
  const past = (t: number) => Math.abs(springValue(t, damping, 1) - 1) >= REST;

  let last = 0;
  for (let t = 8; t > 0; t -= step) {
    if (past(t)) {
      last = t;
      break;
    }
  }
  if (last === 0) return step;

  let lo = last;
  let hi = last + step;
  for (let i = 0; i < 30; i++) {
    const mid = (lo + hi) / 2;
    if (past(mid)) lo = mid;
    else hi = mid;
  }
  return hi;
}

/**
 * A `linear()` easing tracing this spring, plus the duration to run it over.
 *
 * The duration is the FULL settle, which is why it looks long next to the
 * motion it describes: a spring is perceptually done well before it stops.
 *
 * Rounded UP, and then sampled over the rounded value rather than the raw one.
 * Both halves matter: a CSS duration is whole milliseconds, so sampling over
 * anything else puts every keyframe fractionally out of step with the clock
 * that plays it, and rounding down would end the curve a hair short of the
 * target — which reads as the animation stopping just before it lands.
 */
export function springCurve(
  damping: number,
  response: number,
): { curve: string; durationMs: number } {
  const durationMs = Math.ceil(settleRatio(damping) * response * 1000);
  const samples = Array.from({ length: SAMPLES }, (_, i) => {
    // The last sample is the target by definition.
    if (i === SAMPLES - 1) return 1;
    const t = (durationMs / 1000) * (i / (SAMPLES - 1));
    return Number(springValue(t, damping, response).toFixed(4));
  });
  return { curve: `linear(${samples.join(", ")})`, durationMs };
}

/** Peak overshoot as a fraction, for reporting a damping ratio in human terms. */
export function overshoot(damping: number): number {
  if (damping >= 1) return 0;
  return Math.exp((-Math.PI * damping) / Math.sqrt(1 - damping * damping));
}
