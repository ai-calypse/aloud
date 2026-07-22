import { describe, expect, it } from "vitest";
import {
  groupSteps,
  pendingStep,
  withCall,
  withResult,
  type Step,
} from "./agent-steps";
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

/**
 * The reported bug: "send hi to Charlie yang" listed four rows — a contacts
 * search, then "Reading your inbox" twice, then the save — for what the user
 * experienced as one lookup.
 */
describe("groupSteps", () => {
  const step = (name: string, result?: boolean): Step => ({
    name,
    app: "Gmail",
    args: {},
    requiresConfirmation: false,
    result: result === undefined ? undefined : { success: result, message: "" },
  });
  // Stands in for the panel's row text: label plus rendered status.
  const key = (s: Step) => `${s.name}:${s.result ? s.result.success : "running"}`;

  it("stacks adjacent rows that read the same", () => {
    const groups = groupSteps(
      [step("GMAIL_FETCH_EMAILS", true), step("GMAIL_FETCH_EMAILS", true)],
      key,
    );
    expect(groups).toHaveLength(1);
    expect(groups[0].count).toBe(2);
  });

  it("never folds a failure into the success above it", () => {
    const groups = groupSteps(
      [step("GMAIL_FETCH_EMAILS", true), step("GMAIL_FETCH_EMAILS", false)],
      key,
    );
    expect(groups.map((g) => g.count)).toEqual([1, 1]);
  });

  it("reports the last of a run, so a row shows where it got to", () => {
    const last = step("GMAIL_FETCH_EMAILS", true);
    const groups = groupSteps([step("GMAIL_FETCH_EMAILS", true), last], key);
    expect(groups[0].step).toBe(last);
  });

  it("keeps a repeat that is not adjacent — going back to look again is real", () => {
    const groups = groupSteps(
      [
        step("GMAIL_FETCH_EMAILS", true),
        step("GOOGLECONTACTS_SEARCH_CONTACTS", true),
        step("GMAIL_FETCH_EMAILS", true),
      ],
      key,
    );
    expect(groups.map((g) => g.count)).toEqual([1, 1, 1]);
  });

  it("is empty for an empty run", () => {
    expect(groupSteps([], key)).toEqual([]);
  });
});
