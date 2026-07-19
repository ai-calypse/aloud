import { describe, expect, it } from "vitest";
import { BAR_COUNT, FLOOR, pushLevel, restingLevels } from "./meter";

describe("listening meter", () => {
  it("keeps a fixed window, oldest first out", () => {
    let levels = restingLevels();
    expect(levels).toHaveLength(BAR_COUNT);

    levels = pushLevel(levels, 0.9);
    expect(levels).toHaveLength(BAR_COUNT);
    expect(levels[levels.length - 1]).toBeCloseTo(0.9);
  });

  it("attacks instantly — a loud sample is not averaged away", () => {
    const levels = pushLevel(restingLevels(), 1);
    expect(levels[levels.length - 1]).toBe(1);
  });

  /** The bug this guards: bars snapping to silence between syllables. */
  it("releases gradually, not in one step", () => {
    let levels = pushLevel(restingLevels(), 1);
    levels = pushLevel(levels, 0);
    const afterOneQuietSample = levels[levels.length - 1];

    expect(afterOneQuietSample).toBeLessThan(1);
    expect(afterOneQuietSample).toBeGreaterThan(0.5);
  });

  /** The mirror bug: a decay that never lands leaves the row raised forever. */
  it("reaches the floor and stays there through sustained silence", () => {
    let levels = pushLevel(restingLevels(), 1);
    for (let i = 0; i < 40; i++) levels = pushLevel(levels, 0);

    expect(levels.every((v) => v === FLOOR)).toBe(true);
  });

  it("never drops below the floor, so the pill never looks frozen", () => {
    let levels = restingLevels();
    for (const sample of [0, null, 0, null]) levels = pushLevel(levels, sample);
    expect(Math.min(...levels)).toBe(FLOOR);
  });

  it("clamps a level above 1, which would scale a bar past the pill", () => {
    const levels = pushLevel(restingLevels(), 4);
    expect(levels[levels.length - 1]).toBe(1);
  });
});
