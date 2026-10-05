import { describe, expect, it } from "vitest";
import type { AgentSession } from "@/lib/bindings";
import type { Run, Step } from "@/lib/agent-steps";
import { runSummary, sessionLabel, sessionState, stepRole, usageWarning } from "./agents-logic";

const session = (over: Partial<AgentSession> = {}) =>
  ({ id: "a", agent: "claude", title: "", prompt: null, cwd: null, tool: null, busy: false, ...over }) as AgentSession;
const step = (over: Partial<Step> = {}): Step =>
  ({ name: "X", app: "Notes", args: {}, requiresConfirmation: false, ...over }) as Step;
const run = (over: Partial<Run> = {}): Run => ({ heard: "x", steps: [], ...over });

describe("usageWarning", () => {
  it("names a window near its cap in words, as the bar colours it", () => {
    expect(usageWarning(93)).toBe("Nearly full");
    expect(usageWarning(70)).toBe("Getting full");
    expect(usageWarning(69.9)).toBeNull();
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
