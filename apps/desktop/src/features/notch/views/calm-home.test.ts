import { describe, expect, it } from "vitest";
import type { AgentPermission, AgentSession, Battery, FocusSnapshot, Island, NowPlaying, Quota } from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import {
  calmAgents,
  calmBattery,
  calmCalendar,
  calmMedia,
  calmTimer,
  calmUsage,
  dictateHint,
  minutesUntil,
  monthGrid,
  plan,
  weights,
  type FlexKey,
  type Live,
  type Plan,
} from "./calm-home";

const IDLE: Live = { agents: false, timer: false, mediaPlaying: false, charging: false, batteryPercent: 82 };
const area = (p: Plan, key: keyof Plan) => p[key].w * p[key].h;
const FLEX: FlexKey[] = ["media", "timer", "agents", "battery", "calendar", "weather"];

describe("weights", () => {
  it("pins a critical battery above everything, and only while it is not charging", () => {
    expect(weights({ ...IDLE, batteryPercent: 9 }).battery).toBe(10);
    expect(weights({ ...IDLE, batteryPercent: 9, charging: true }).battery).toBe(4);
    expect(weights({ ...IDLE, batteryPercent: 10 }).battery).toBe(3);
  });

  it("ranks agents over the timer over music, and music only while it plays", () => {
    const w = weights({ ...IDLE, agents: true, timer: true, mediaPlaying: true });
    expect([w.agents, w.timer, w.media]).toEqual([9, 7, 6]);
    expect(weights(IDLE)).toMatchObject({ agents: 1, timer: 1, media: 1, battery: 1.2, calendar: 3.5, weather: 1.5 });
  });

  it("weighs a Mac with no battery as a healthy one", () => {
    expect(weights({ ...IDLE, batteryPercent: null }).battery).toBe(1.2);
  });
});

describe("plan", () => {
  // Everything the app can produce: the weights are a finite set, so the
  // "no layout fits" throw in plan() is provably unreachable.
  const everyLive: Live[] = [];
  for (const agents of [false, true])
    for (const timer of [false, true])
      for (const mediaPlaying of [false, true])
        for (const [charging, batteryPercent] of [[false, 5], [true, 50], [false, 25], [false, 80], [false, null]] as const)
          everyLive.push({ agents, timer, mediaPlaying, charging, batteryPercent });

  it("fills the 8x3 grid with no overlap and no gap, for every state the app can be in", () => {
    for (const live of everyLive) {
      const p = plan(weights(live));
      const grid = Array.from({ length: 3 }, () => Array<number>(8).fill(0));
      for (const t of Object.values(p)) {
        expect(t.col + t.w - 1, JSON.stringify(live)).toBeLessThanOrEqual(8);
        expect(t.row + t.h - 1, JSON.stringify(live)).toBeLessThanOrEqual(3);
        for (let y = t.row - 1; y < t.row - 1 + t.h; y++)
          for (let x = t.col - 1; x < t.col - 1 + t.w; x++) grid[y][x]++;
      }
      expect(grid.flat().every((n) => n === 1), JSON.stringify(live)).toBe(true);
      // The six that resize take exactly 22 cells; Dictate and New note the last 2.
      expect(FLEX.reduce((n, k) => n + area(p, k), 0)).toBe(22);
    }
  });

  it("puts the heaviest live tile first, top-left, at its large size", () => {
    expect(plan(weights({ ...IDLE, agents: true })).agents).toMatchObject({ col: 1, row: 1, size: "L" });
  });

  // First-fit in weight order dead-ends for a live timer with live music (the
  // prototype silently kept the previous layout); backing up finds one, and a
  // state first-fit solved keeps the layout first-fit gave it.
  it("lays out the states plain first-fit dead-ends on, the heavier live tile leading", () => {
    const p = plan(weights({ ...IDLE, timer: true, mediaPlaying: true }));
    expect(p.timer).toMatchObject({ col: 1, row: 1 });
    expect(p.media.size).toBe("L");
    expect(plan(weights({ ...IDLE, agents: true, mediaPlaying: true })).agents).toMatchObject({ col: 1, row: 1, size: "L" });
  });

  it("makes a critical battery a tall tile in the first cell", () => {
    expect(plan(weights({ ...IDLE, batteryPercent: 5 })).battery).toMatchObject({ col: 1, row: 1, w: 1, h: 2, size: "L" });
  });

  // The idle board is the one a new Mac sees: the calendar leads.
  it("lets the calendar lead when nothing is live", () => {
    expect(plan(weights(IDLE)).calendar).toMatchObject({ col: 1, row: 1, size: "L" });
  });

  it("shrinks tiles to make room when everything is live", () => {
    const idle = plan(weights(IDLE));
    const busy = plan(weights({ agents: true, timer: true, mediaPlaying: true, charging: false, batteryPercent: 5 }));
    expect(area(busy, "calendar")).toBeLessThan(area(idle, "calendar"));
    expect(area(busy, "weather")).toBeLessThanOrEqual(area(idle, "weather"));
    for (const k of ["agents", "timer", "media", "battery"] as const) expect(busy[k].size).toBe("L");
  });

  it("is the same plan for the same weights, so a re-render never reshuffles", () => {
    expect(plan(weights(IDLE))).toEqual(plan(weights(IDLE)));
  });
});

const battery = (percent: number, state: Battery["state"]): Battery => ({
  percent,
  state,
  minutes: null,
  cycles: null,
  health: null,
});

describe("calmBattery", () => {
  it("is green and Good while healthy", () => {
    expect(calmBattery(battery(82, "discharging"))).toMatchObject({ word: "Good", state: "ok" });
  });

  // The word moves with the colour: yellow always says Low, red always Critical.
  it("says Low in caution at 20% and under, Critical in alert under 10%", () => {
    expect(calmBattery(battery(21, "discharging"))).toMatchObject({ word: "Good", state: "ok" });
    expect(calmBattery(battery(20, "discharging"))).toMatchObject({ word: "Low", state: "caution" });
    expect(calmBattery(battery(10, "discharging"))).toMatchObject({ word: "Low", state: "caution" });
    expect(calmBattery(battery(9, "discharging"))).toMatchObject({ word: "Critical", state: "alert" });
  });

  it("stays green and says what it is doing on the charger, even when low", () => {
    expect(calmBattery(battery(8, "charging"))).toMatchObject({ word: "Charging", state: "ok", charging: true });
    expect(calmBattery(battery(8, "pluggedIn"))).toMatchObject({ word: "Plugged in", state: "ok" });
    expect(calmBattery(battery(100, "full")).word).toBe("Full");
  });
});

const session = (over: Partial<AgentSession>): AgentSession =>
  ({ id: "s", agent: "claude", title: "", prompt: null, cwd: "/Users/me/aloud-tauri", tool: null, busy: false, ...over }) as AgentSession;
const NAMES = { claude: "Claude Code", codex: "Codex" };

describe("calmAgents", () => {
  const sessions = [
    session({ id: "idle", agent: "codex", cwd: "/x/website" }),
    session({ id: "run", agent: "codex", cwd: "/x/site", busy: true }),
    session({ id: "ask" }),
  ];
  const asking = [{ id: "ask", sessionId: "ask" } as AgentPermission];

  it("lists what needs the user first, then what is working, then what is idle", () => {
    const a = calmAgents(sessions, asking, null, NAMES);
    expect(a.rows.map((r) => [r.id, r.state])).toEqual([["ask", "wait"], ["run", "run"], ["idle", "idle"]]);
    expect(a.rows[0]).toMatchObject({ name: "Claude Code · aloud-tauri", detail: "Wants permission" });
  });

  it("counts what is active, and says who needs you in a word", () => {
    expect(calmAgents(sessions, asking, null, NAMES)).toMatchObject({ active: 2, needs: 1, word: "1 needs you" });
    expect(calmAgents(sessions, [], null, NAMES)).toMatchObject({ active: 1, needs: 0, word: "1 running" });
  });

  it("falls back to the total when everything is idle, so the tile still has a number", () => {
    expect(calmAgents([session({})], [], null, NAMES)).toMatchObject({ active: 1, word: "all idle" });
  });

  it("shows a finished voice command with a green tick and a failed one in red", () => {
    const run = (over: Partial<Run>): Run => ({ heard: "email David", steps: [], ...over });
    const done = calmAgents([], [], run({ finished: "Sent" }), NAMES).rows[0];
    expect(done).toMatchObject({ name: "Voice · email David", state: "done", detail: "Done" });
    expect(calmAgents([], [], run({ error: "no" }), NAMES).rows[0].state).toBe("fail");
  });

  it("has no rows, and so an empty tile, with nothing running", () => {
    expect(calmAgents([], [], null, NAMES).rows).toEqual([]);
  });
});

describe("calmUsage", () => {
  const quota = (...p: number[]) =>
    ({
      fiveHour: { percent: p[0], resetsAt: null },
      sevenDay: p[1] === undefined ? null : { percent: p[1], resetsAt: null },
      sevenDayOpus: null,
      extraUsagePercent: null,
    }) as Quota;

  it("reads the fullest window and warns where the Usage view does", () => {
    expect(calmUsage(quota(30, 62))).toEqual({ percent: 62, word: null, state: undefined });
    expect(calmUsage(quota(30, 72))).toMatchObject({ word: "Getting full", state: "caution" });
    expect(calmUsage(quota(95))).toMatchObject({ word: "Nearly full", state: "alert" });
  });

  it("is absent until the plan has been read", () => {
    expect(calmUsage(null)).toBeNull();
  });
});

const MINS = 60_000;
const timer = (over: Partial<FocusSnapshot["timer"]>): FocusSnapshot => ({
  awake: { active: false, until: null, minutes: null },
  timer: {
    mode: "pomodoro",
    phase: "focus",
    status: "running",
    round: 2,
    rounds: 4,
    preset: { focus: 25, rest: 5 },
    at: 19 * MINS + 42_000,
    heldMs: 0,
    totalMs: 25 * MINS,
    ...over,
  } as FocusSnapshot["timer"],
});

describe("calmTimer", () => {
  it("says where in the set it is, and offers Pause while running", () => {
    expect(calmTimer(timer({}), 0)).toMatchObject({ clock: "19:42", chip: "2 of 4", minutes: "20", rounds: { done: 2, of: 4 }, button: { label: "Pause", action: "focus.pause" } });
  });

  it("offers Resume when paused and Start when a phase is waiting", () => {
    expect(calmTimer(timer({ status: "paused", heldMs: 5 * MINS }), 0).button).toEqual({ label: "Resume", action: "focus.resume" });
    expect(calmTimer(timer({ status: "ready", heldMs: 25 * MINS }), 0).button?.label).toBe("Start");
  });

  it("has no session count and no ring to drain for a stopwatch", () => {
    const t = calmTimer(timer({ mode: "stopwatch", at: 0 }), 90_000);
    expect(t).toMatchObject({ chip: null, rounds: null, clock: "1:30", minutes: "1", value: 0 });
  });

  it("is idle with nothing to press", () => {
    expect(calmTimer(null, 0)).toMatchObject({ idle: true, button: null, chip: null });
  });
});

describe("calmMedia", () => {
  const track = { title: "Night Drive", artist: "Halcyon", state: "playing", durationMs: 222_000 } as NowPlaying;

  it("says the position and the time left, and how far along it is", () => {
    expect(calmMedia(track, 102_000)).toMatchObject({ position: "1:42", left: "-2:00", playing: true, scrub: true });
    expect(calmMedia(track, 111_000).progress).toBeCloseTo(0.5);
  });

  it("does not offer a scrubber for a video with no length", () => {
    expect(calmMedia({ ...track, durationMs: 0 }, 5_000)).toMatchObject({ scrub: false, progress: 0, left: "" });
  });
});

describe("calendar", () => {
  const island = (detail: string): Island =>
    ({ id: "calendar:next", title: "Design sync", detail, kind: "live" }) as Island;
  const now = new Date(2026, 9, 4, 14, 5).getTime();

  it("reads the island's two phrasings, and nothing else", () => {
    expect(minutesUntil("in 25 min")).toBe(25);
    expect(minutesUntil("now")).toBe(0);
    expect(minutesUntil("tomorrow")).toBeNull();
  });

  it("fills the countdown over the hour before the event", () => {
    expect(calmCalendar([island("in 15 min")], now, "en-US")).toMatchObject({ minutes: 15, progress: 0.75 });
    expect(calmCalendar([island("now")], now, "en-US").progress).toBe(1);
    expect(calmCalendar([island("in 25 min")], now, "en-US").next).toEqual({ title: "Design sync", when: "in 25 min" });
  });

  it("is only a date, with no made-up event, when the island has nothing", () => {
    expect(calmCalendar([], now, "en-US")).toMatchObject({ next: null, minutes: null, weekday: "Sunday", day: "4" });
  });

  it("lays out October 2026, Monday first: 1 Oct is a Thursday and today is a Sunday", () => {
    const m = monthGrid(now, "en-US");
    expect(m.month).toBe("Oct");
    expect(m.cells.slice(0, 3)).toEqual([null, null, null]);
    expect(m.cells[3]).toMatchObject({ day: 1, weekend: false });
    expect(m.cells.filter((c) => c?.today)).toEqual([{ day: 4, today: true, weekend: true }]);
    expect(m.cells.filter(Boolean)).toHaveLength(31);
  });
});

describe("dictateHint", () => {
  const key = (k: string) => ({ kind: "modifierKey", key: k, modifiers: [] }) as never;
  it("names the user's own key, not a built-in", () => {
    expect(dictateHint({ hold: key("fn") } as never)).toBe("Hold Fn 🌐");
  });
  it("falls back to the toggle key, then admits there is none", () => {
    expect(dictateHint({ hold: { kind: "disabled" }, toggle: key("fn") } as never)).toBe("Press Fn 🌐");
    expect(dictateHint({ hold: null, toggle: null } as never)).toBe("No hotkey set");
    expect(dictateHint(undefined)).toBe("No hotkey set");
  });
});
