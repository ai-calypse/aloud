import { describe, expect, it } from "vitest";
import type { AgentSession, Battery, FocusSnapshot, Island } from "@/lib/bindings";
import { agentsWidget, batteryWidget, calendarWidget, timerWidget } from "./widget-home";

const MIN = 60_000;
const battery = (percent: number, state: Battery["state"]): Battery => ({
  percent,
  state,
  minutes: null,
  cycles: null,
  health: null,
});

describe("batteryWidget", () => {
  it("is green and says what it is doing while healthy", () => {
    expect(batteryWidget(battery(82, "discharging"))).toMatchObject({
      value: 0.82,
      text: "82",
      word: "On battery",
      accent: "ok",
    });
    expect(batteryWidget(battery(40, "charging")).word).toBe("Charging");
  });

  // Colour is never the only carrier: the word moves with the accent.
  it("goes amber then red as it runs out, each with a word", () => {
    expect(batteryWidget(battery(18, "discharging"))).toMatchObject({ word: "Low", accent: "orange" });
    expect(batteryWidget(battery(8, "discharging"))).toMatchObject({ word: "Very low", accent: "move" });
  });

  it("stays green when low but on the charger", () => {
    expect(batteryWidget(battery(8, "charging")).accent).toBe("ok");
  });
});

const snapshot = (timer: Partial<FocusSnapshot["timer"]>): FocusSnapshot => ({
  awake: { active: false, until: null, minutes: null },
  timer: {
    mode: "pomodoro",
    phase: "focus",
    status: "running",
    round: 1,
    rounds: 4,
    preset: { focus: 25, rest: 5 },
    at: 25 * MIN,
    heldMs: 0,
    totalMs: 25 * MIN,
    ...timer,
  } as FocusSnapshot["timer"],
});

describe("timerWidget", () => {
  it("is empty and says nothing is running when idle or unread", () => {
    expect(timerWidget(null, 0)).toMatchObject({ idle: true, running: false, value: 0 });
    expect(timerWidget(snapshot({ status: "idle" }), 0).idle).toBe(true);
  });

  it("drains: the ring is what is left of the phase", () => {
    // 10 of 25 minutes gone.
    const w = timerWidget(snapshot({ at: 25 * MIN }), 10 * MIN);
    expect(w.text).toBe("15:00");
    expect(w.value).toBeCloseTo(0.6);
    expect(w).toMatchObject({ word: "Focus", accent: "move", running: true });
  });

  it("holds its value and says Paused", () => {
    const w = timerWidget(snapshot({ status: "paused", heldMs: 5 * MIN }), 99 * MIN);
    expect(w).toMatchObject({ text: "5:00", word: "Paused", running: false });
    expect(w.value).toBeCloseTo(0.2);
  });

  it("moves to the exercise ring on a break", () => {
    expect(timerWidget(snapshot({ phase: "shortBreak", totalMs: 5 * MIN, at: 5 * MIN }), 0)).toMatchObject({
      word: "Break",
      accent: "exercise",
    });
  });

  it("gives a stopwatch no ring to drain", () => {
    const w = timerWidget(snapshot({ mode: "stopwatch", at: 0, totalMs: 0 }), 90_000);
    expect(w).toMatchObject({ text: "1:30", word: "Stopwatch", value: 0 });
  });
});

const agent = (busy: boolean): AgentSession => ({ id: String(Math.random()), busy }) as AgentSession;

describe("agentsWidget", () => {
  it("leads with what needs the user", () => {
    expect(agentsWidget([agent(true)], [{} as never, {} as never])).toEqual({
      count: 2,
      word: "waiting for you",
      live: true,
    });
  });

  it("counts the busy ones, and says so plainly when there are none", () => {
    expect(agentsWidget([agent(true), agent(false), agent(true)], [])).toMatchObject({ count: 2, word: "running" });
    expect(agentsWidget([agent(false)], [])).toEqual({ count: null, word: "all idle", live: false });
    expect(agentsWidget([], [])).toEqual({ count: null, word: "no agents", live: false });
  });
});

const island = (id: string, title: string, detail: string | null): Island => ({
  id,
  kind: "live",
  priority: "normal",
  icon: null,
  title,
  detail,
  actions: [],
  expires: null,
  dismissible: false,
});

describe("calendarWidget", () => {
  const now = new Date(2026, 9, 4, 14, 5).getTime(); // Sunday 4 October 2026

  it("takes the date from the clock", () => {
    expect(calendarWidget([], now, "en-US")).toEqual({ weekday: "Sunday", day: "4", next: null });
  });

  it("shows the next event only when the island carries one, and never invents one", () => {
    const w = calendarWidget([island("agents", "x", null), island("calendar:next", "Design sync", "in 25 min")], now, "en-US");
    expect(w.next).toEqual({ title: "Design sync", when: "in 25 min" });
    expect(calendarWidget([island("agents", "x", null)], now, "en-US").next).toBeNull();
  });
});
