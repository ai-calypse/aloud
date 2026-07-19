/**
 * The listening meter's sample buffer.
 *
 * Levels arrive from Rust every 50ms as a 0–1 loudness already put through a
 * perceptual curve. This turns that stream into the bar heights the overlay
 * draws: a fixed-width window of history that scrolls, with the attack/release
 * shaping that makes a level meter look alive rather than jittery.
 *
 * Separate from the component because it is the part that can be quietly
 * wrong — a decay that never reaches the floor leaves the bars permanently
 * raised, and one that decays too fast strobes between syllables. Neither
 * shows up in a screenshot.
 */

/** Bars in the meter — at 50ms a sample, ~700ms of history. */
export const BAR_COUNT = 14;

/**
 * Fraction of its height a bar keeps when the next sample is quieter.
 *
 * Real meters attack instantly and release slowly. 0.72 at 50ms decays a peak
 * to the floor over roughly 300ms — long enough to ride through the gap
 * between syllables, short enough to still track a sentence.
 */
export const RELEASE = 0.72;

/** Silence still shows a stub, so the pill reads as listening, not frozen. */
export const FLOOR = 0.12;

/** A flat resting row, for before the first sample of a session arrives. */
export function restingLevels(): number[] {
  return Array.from({ length: BAR_COUNT }, () => FLOOR);
}

/**
 * Appends one sample, dropping the oldest.
 *
 * The new bar is the louder of what was just heard and a decayed copy of the
 * bar before it, so a peak falls away over several frames instead of snapping
 * back to silence the moment the speaker pauses for breath.
 */
export function pushLevel(previous: number[], heard: number | null): number[] {
  const level = Math.max(FLOOR, Math.min(1, heard ?? 0));
  const decayed = (previous[previous.length - 1] ?? FLOOR) * RELEASE;
  return [...previous.slice(1), Math.max(level, decayed)];
}
