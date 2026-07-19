import { describe, expect, it } from "vitest";
import { pendingStep, withCall, withResult, type Step } from "./agent-steps";
import type { ToolCall, ToolResult } from "./bindings";

const call = (name: string, requiresConfirmation: boolean): ToolCall => ({
  name,
  app: "gmail",
  args: { subject: name, body: "hello", user_id: "me" },
  requiresConfirmation,
});
const ok: ToolResult = { success: true, message: "done" };

/** Rust emits the call, then — for a write — the same call again with the gate. */
function startAndGate(steps: Step[], name: string): Step[] {
  return withCall(withCall(steps, call(name, false)), call(name, true));
}

describe("agent panel step folding", () => {
  it("folds the start and gate events for one call into a single step", () => {
    const steps = startAndGate([], "GMAIL_SEND_EMAIL");
    expect(steps).toHaveLength(1);
    expect(steps[0].requiresConfirmation).toBe(true);
  });

  /**
   * The regression this file exists for. "Send this and draft a reply" is two
   * writes in one run. The panel used to keep a single call and result, so the
   * second call inherited the first one's result, its gate never rendered, and
   * the write sat waiting on a button that was never drawn.
   */
  it("still opens a gate for a second write after the first one finished", () => {
    let steps = startAndGate([], "GMAIL_SEND_EMAIL");
    steps = steps.map((s) => ({ ...s, confirmed: true }));
    steps = withResult(steps, ok);

    steps = startAndGate(steps, "GMAIL_CREATE_EMAIL_DRAFT");

    expect(steps).toHaveLength(2);
    expect(steps[0].result).toEqual(ok);
    const pending = pendingStep({ heard: "", steps });
    expect(pending?.name).toBe("GMAIL_CREATE_EMAIL_DRAFT");
  });

  it("attaches a result to the step still waiting, not the finished one", () => {
    let steps = startAndGate([], "A");
    steps = withResult(steps, ok);
    steps = withCall(steps, call("B", false));
    steps = withResult(steps, { success: false, message: "boom" });

    expect(steps[0].result).toEqual(ok);
    expect(steps[1].result?.success).toBe(false);
  });

  it("treats a declined step as settled so it cannot re-open the gate", () => {
    let steps = startAndGate([], "GMAIL_SEND_EMAIL");
    steps = steps.map((s) => ({ ...s, confirmed: false }));
    expect(pendingStep({ heard: "", steps })).toBeUndefined();
  });
});
