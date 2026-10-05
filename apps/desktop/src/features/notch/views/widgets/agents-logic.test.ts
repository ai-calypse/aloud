import { describe, expect, it } from "vitest";
import type { AgentSession, Quota } from "@/lib/bindings";
import type { Run, Step } from "@/lib/agent-steps";
import { extraUsage, runSummary, sessionLabel, sessionState, stepRole, usageBars, usageWarning } from "./agents-logic";

const NOW = Date.parse("2026-10-04T12:00:00Z");
const session = (over: Partial<AgentSession> = {}) =>
  ({ id: "a", agent: "claude", title: "", prompt: null, cwd: null, tool: null, busy: false, ...over }) as AgentSession;
const step = (over: Partial<Step> = {}): Step =>
  ({ name: "X", app: "Notes", args: {}, requiresConfirmation: false, ...over }) as Step;
const run = (over: Partial<Run> = {}): Run => ({ heard: "x", steps: [], ...over });

describe("usageBars", () => {
  const quota: Quota = {
    fiveHour: { percent: 42.4, resetsAt: "2026-10-04T14:10:00Z" },
    sevenDay: { percent: 93, resetsAt: null },
    sevenDayOpus: null,
    extraUsagePercent: null,
  };

  it("keeps each window on its own Screen Time colour and skips the ones the plan left out", () => {
    const bars = usageBars(quota, NOW);
    expect(bars.map((b) => [b.label, b.role])).toEqual([
      ["5-hour window", "blue"],
      ["Week", "cyan"],
    ]);
  });

  it("gives each window a heading short enough for a four-column row", () => {
    expect(usageBars({ ...quota, sevenDayOpus: { percent: 5, resetsAt: null } }, NOW).map((b) => b.short)).toEqual([
      "5-hour",
      "Week",
      "Opus",
    ]);
  });

  it("rounds to the whole percent the words say and keeps the bar on the same number", () => {
    const [five] = usageBars(quota, NOW);
    expect(five.percent).toBe(42);
    expect(five.value).toBe(0.42);
    expect(five.resets).toBe("Resets in 2h");
  });

  it("names a window near its cap in words, as the calm view colours it", () => {
    const [five, week] = usageBars(quota, NOW);
    expect(five.warning).toBeNull();
    expect(week.warning).toBe("Nearly full");
    expect(week.resets).toBeNull();
    expect(usageWarning(70)).toBe("Getting full");
    expect(usageWarning(69.9)).toBeNull();
  });

  it("clamps what the API sends out of range, like the calm bar", () => {
    const q = { ...quota, fiveHour: { percent: 140, resetsAt: null } };
    expect(usageBars(q, NOW)[0].percent).toBe(100);
  });

  it("is empty before the first read", () => {
    expect(usageBars(null, NOW)).toEqual([]);
  });
});

describe("extraUsage", () => {
  it("shows spend past the plan only when there is some", () => {
    const base = { fiveHour: null, sevenDay: null, sevenDayOpus: null };
    expect(extraUsage({ ...base, extraUsagePercent: 0 })).toBeNull();
    expect(extraUsage({ ...base, extraUsagePercent: null })).toBeNull();
    expect(extraUsage({ ...base, extraUsagePercent: 12.6 })).toBe(13);
  });
});

describe("sessionState", () => {
  it("puts a permission request first, whatever the agent is doing", () => {
    const s = sessionState(session({ busy: true, tool: "Bash: cargo test" }), true);
    expect(s).toMatchObject({ word: "Wants permission", role: "orange", detail: "Bash: cargo test" });
  });

  it("says Running with a tool, Thinking without one, Idle when nothing is happening", () => {
    expect(sessionState(session({ busy: true, tool: "Edit: a.rs" }), false).word).toBe("Running");
    expect(sessionState(session({ busy: true }), false).word).toBe("Thinking");
    const idle = sessionState(session({ prompt: "fix the build" }), false);
    expect(idle).toMatchObject({ word: "Idle", role: undefined, detail: "You: fix the build" });
  });

  it("falls back to the model's title when there is no prompt", () => {
    expect(sessionState(session({ title: "Refactor hotkeys" }), false).detail).toBe("Refactor hotkeys");
  });
});

describe("sessionLabel", () => {
  it("is the folder, else the agent's name", () => {
    expect(sessionLabel(session({ cwd: "/Users/y/dev/aloud-tauri/" }), {})).toBe("aloud-tauri");
    expect(sessionLabel(session(), { claude: "Claude Code" })).toBe("Claude Code");
  });
});

describe("runSummary", () => {
  it("counts a queued step in the total but not in the done", () => {
    const r = run({
      steps: [step({ result: { success: true } as Step["result"] }), step()],
      planned: [{ name: "NEXT" } as never],
    });
    expect(runSummary(r)).toMatchObject({ done: 1, total: 3, word: "Working" });
  });

  it("counts a declined step as settled", () => {
    expect(runSummary(run({ steps: [step({ confirmed: false })] })).done).toBe(1);
  });

  it("asks for the yes while a gate is open, ahead of everything but failure", () => {
    const gate = step({ requiresConfirmation: true });
    expect(runSummary(run({ steps: [gate] })).word).toBe("Needs your yes");
    expect(runSummary(run({ steps: [gate], error: "boom" })).word).toBe("Failed");
  });

  it("says Asking you for a question and Done only once the answer is in", () => {
    expect(runSummary(run({ question: { question: "Which?", options: [] } })).word).toBe("Asking you");
    expect(runSummary(run({ finished: "ok" }))).toMatchObject({ word: "Done", role: "ok" });
  });
});

describe("stepRole", () => {
  it("marks a failure and a waiting gate, and a decline is neutral", () => {
    expect(stepRole(step({ result: { success: false } as Step["result"] }), false)).toBe("move");
    expect(stepRole(step(), true)).toBe("orange");
    expect(stepRole(step({ confirmed: false, result: { success: false } as Step["result"] }), false)).toBeUndefined();
    expect(stepRole(step({ result: { success: true } as Step["result"] }), false)).toBeUndefined();
  });
});
