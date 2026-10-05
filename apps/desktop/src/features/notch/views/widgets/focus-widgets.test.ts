import { describe, expect, it } from "vitest";
import type { AwakeState, FocusDay, TimerState } from "@/lib/bindings";
import {
  awakeChoice,
  noteSnippet,
  noteTitle,
  noteWhen,
  primaryLabel,
  timerCaption,
  timerFace,
  weekWidget,
} from "./focus-widgets";

const MIN = 60_000;
const timer = (over: Partial<TimerState> = {}): TimerState => ({
  mode: "pomodoro",
  phase: "focus",
  status: "running",
  round: 2,
  rounds: 4,
  preset: { focus: 25, rest: 5 },
  at: 1_000_000 + 10 * MIN,
  heldMs: 0,
  totalMs: 25 * MIN,
  ...over,
});

describe("timerFace", () => {
  it("drains: the ring is what is left, so ten of twenty-five minutes is 40%", () => {
    const f = timerFace(timer(), 1_000_000, "pomodoro", 25);
    expect(f).toMatchObject({ idle: false, countdown: true, text: "10:00", title: "Focus 2 of 4", accent: "move" });
    expect(f.value).toBeCloseTo(0.4);
  });

  it("shows the chosen preset full while idle", () => {
    const f = timerFace(undefined, 0, "pomodoro", 50);
    expect(f).toMatchObject({ idle: true, countdown: true, text: "50:00", value: 1, title: "Focus" });
  });

  it("turns Exercise green on a break", () => {
    const f = timerFace(timer({ phase: "shortBreak" }), 1_000_000, "pomodoro", 25);
    expect(f.accent).toBe("exercise");
    expect(f.title).toBe("Short break");
  });

  // A stopwatch has no end to drain towards.
  it("counts a stopwatch up with no ring value to trust", () => {
    const f = timerFace(timer({ mode: "stopwatch", at: 1_000_000 - 65_000 }), 1_000_000, "pomodoro", 25);
    expect(f).toMatchObject({ countdown: false, text: "1:05", title: "Stopwatch" });
  });

  it("holds the time while paused", () => {
    const f = timerFace(timer({ status: "paused", heldMs: 5 * MIN }), 9_999_999, "pomodoro", 25);
    expect(f.text).toBe("5:00");
    expect(f.value).toBeCloseTo(0.2);
  });
});

describe("timer words", () => {
  it("names the primary button for each state, as a Start/Pause/Resume button should", () => {
    expect(primaryLabel(undefined)).toBe("Start");
    expect(primaryLabel(timer({ status: "idle" }))).toBe("Start");
    expect(primaryLabel(timer())).toBe("Pause");
    expect(primaryLabel(timer({ status: "paused" }))).toBe("Resume");
    expect(primaryLabel(timer({ status: "ready" }))).toBe("Start focus");
    expect(primaryLabel(timer({ status: "ready", phase: "shortBreak" }))).toBe("Start break");
  });

  it("captions what is happening", () => {
    expect(timerCaption(timer())).toBe("25 minutes of focus, then 5 to rest.");
    expect(timerCaption(timer({ status: "paused" }))).toBe("Paused.");
    expect(timerCaption(timer({ mode: "stopwatch" }))).toBe("Counting up.");
    expect(timerCaption(timer({ status: "ready" }))).toBe("Focus 2 of 4 is ready when you are.");
  });
});

const days = (minutes: number[]): FocusDay[] =>
  minutes.map((m, i) => ({ day: `2026-10-0${i + 1}`, sessions: m > 0 ? 1 : 0, minutes: m }));

describe("weekWidget", () => {
  it("keeps an hour of headroom so one short session is not a full bar", () => {
    const w = weekWidget(days([0, 0, 0, 0, 0, 0, 30]), 1);
    expect(w.bars[6].height).toBeCloseTo(0.5);
  });

  it("scales to the busiest day once it passes an hour", () => {
    const w = weekWidget(days([120, 60, 0, 0, 0, 0, 30]), 0);
    expect(w.bars[0].height).toBe(1);
    expect(w.bars[1].height).toBe(0.5);
  });

  it("gives a day with any focus a sliver and an empty day nothing", () => {
    const w = weekWidget(days([1, 0, 0, 0, 0, 0, 0]), 0);
    expect(w.bars[0].height).toBe(0.12);
    expect(w.bars[1].height).toBe(0);
  });

  it("marks today last, and says the totals in words", () => {
    const w = weekWidget(days([25, 25, 0, 0, 0, 0, 50]), 3);
    expect(w.bars.map((b) => b.today)).toEqual([false, false, false, false, false, false, true]);
    expect(w).toMatchObject({ total: 100, totalWord: "1h 40m this week", streakWord: "day streak", todaySessions: 1 });
  });

  it("says plainly when there is nothing yet", () => {
    const w = weekWidget(days([0, 0, 0, 0, 0, 0, 0]), 0);
    expect(w).toMatchObject({ totalWord: "None this week", streakWord: "No streak yet", todaySessions: 0 });
  });

  it("labels each day by its weekday", () => {
    // 2026-10-04 is a Sunday.
    expect(weekWidget(days([0, 0, 0, 0, 0, 0, 0]), 0).bars.map((b) => b.initial)).toEqual([
      "T", "F", "S", "S", "M", "T", "W",
    ]);
  });
});

describe("awakeChoice", () => {
  const a = (over: Partial<AwakeState>): AwakeState => ({ active: true, until: null, minutes: null, ...over });
  it("reads which choice is live after the panel is reopened", () => {
    expect(awakeChoice(undefined)).toBe("off");
    expect(awakeChoice(a({ active: false }))).toBe("off");
    expect(awakeChoice(a({ minutes: 120 }))).toBe("120");
    expect(awakeChoice(a({ minutes: null }))).toBe("always");
  });
});

describe("notes text", () => {
  it("titles a note by its first real line, without markdown's #", () => {
    expect(noteTitle("\n\n## Call prep\nbody")).toBe("Call prep");
    expect(noteTitle("   \n")).toBe("Empty note");
  });

  it("previews the line after the title", () => {
    expect(noteSnippet("Title\n\nOpen with the usage numbers\nmore")).toBe("Open with the usage numbers");
    expect(noteSnippet("Only a title")).toBe("");
  });

  it("dates a note as Notes does", () => {
    const now = new Date(2026, 9, 4, 15, 0).getTime();
    const at = (y: number, m: number, d: number, h = 9) => new Date(y, m, d, h, 5).getTime() / 1000;
    expect(noteWhen(at(2026, 9, 4), now)).toMatch(/9:05/);
    expect(noteWhen(at(2026, 9, 3), now)).toBe("Yesterday");
    expect(noteWhen(at(2026, 8, 20), now)).not.toBe("Yesterday");
  });
});
