import { describe, expect, it } from "vitest";
import type { TimerState } from "@/lib/bindings";
import { clock, nextTickIn, timerTitle, timerValueMs } from "./focus";

const MIN = 60_000;
const pomodoro = (over: Partial<TimerState> = {}): TimerState => ({
  mode: "pomodoro",
  phase: "focus",
  status: "running",
  round: 2,
  rounds: 4,
  preset: { focus: 25, rest: 5 },
  at: 25 * MIN,
  heldMs: 0,
  totalMs: 25 * MIN,
  ...over,
});

describe("timerValueMs", () => {
  // The wall clock is the only input: ten minutes of sleep is ten minutes gone.
  it("derives the time left from the clock, whatever happened in between", () => {
    expect(timerValueMs(pomodoro(), 10 * MIN)).toBe(15 * MIN);
    expect(timerValueMs(pomodoro(), 20 * MIN)).toBe(5 * MIN);
  });

  it("never goes negative past the end or above the phase length", () => {
    expect(timerValueMs(pomodoro(), 99 * MIN)).toBe(0);
    expect(timerValueMs(pomodoro(), -5 * MIN)).toBe(25 * MIN);
  });

  it("holds the value while paused or waiting", () => {
    expect(timerValueMs(pomodoro({ status: "paused", heldMs: 7 * MIN }), 999 * MIN)).toBe(7 * MIN);
  });

  it("counts a stopwatch up from when it began", () => {
    const s = pomodoro({ mode: "stopwatch", at: 1000, totalMs: 0 });
    expect(timerValueMs(s, 1000 + 90_000)).toBe(90_000);
  });
});

describe("clock", () => {
  it("rounds a countdown up so the last second reads 0:01", () => {
    expect(clock(25 * MIN, true)).toBe("25:00");
    expect(clock(1, true)).toBe("0:01");
    expect(clock(0, true)).toBe("0:00");
    expect(clock(3_660_000, true)).toBe("1:01:00");
  });
  it("floors a stopwatch", () => {
    expect(clock(59_999, false)).toBe("0:59");
  });
});

describe("nextTickIn", () => {
  // A digit flips when the remaining time crosses a whole second: 13:27.250
  // left shows 13:28 until it reaches 13:27.000, 250ms away.
  it("lands on the next change of the face", () => {
    const s = pomodoro({ at: 1_250 });
    expect(nextTickIn(s, 0)).toBe(250 + 4);
    expect(nextTickIn(pomodoro({ at: 2_000 }), 0)).toBe(1000 + 4);
  });
  it("counts a stopwatch to its next whole second", () => {
    const s = pomodoro({ mode: "stopwatch", at: 0 });
    expect(nextTickIn(s, 1_750)).toBe(250 + 4);
  });
});

describe("timerTitle", () => {
  it("names the place in the session", () => {
    expect(timerTitle(pomodoro())).toBe("Focus 2 of 4");
    expect(timerTitle(pomodoro({ phase: "longBreak" }))).toBe("Long break");
    expect(timerTitle(pomodoro({ mode: "stopwatch" }))).toBe("Stopwatch");
  });
});
