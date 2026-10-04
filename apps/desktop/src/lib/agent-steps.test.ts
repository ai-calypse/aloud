import { describe, expect, it } from "vitest";
import {
  groupSteps,
  pendingStep,
  stepStatus,
  withCall,
  withDecision,
  withResult,
  type Step,
} from "./agent-steps";
import type { ActionPreview, ToolCall, ToolResult } from "./bindings";

const call = (name: string, requiresConfirmation: boolean): ToolCall => ({
  name,
  app: "gmail",
  args: { subject: name, body: "hello", user_id: "me" },
  requiresConfirmation,
  preview: null,
  canAlways: false,
});
const ok: ToolResult = { success: true, message: "done", show: null };

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
    steps = withResult(steps, { success: false, message: "boom", show: null });

    expect(steps[0].result).toEqual(ok);
    expect(steps[1].result?.success).toBe(false);
  });

  /** The finished row's Show button reads this off the step's result. */
  it("keeps what a finished step can show", () => {
    const show = { app: "Notes", target: "x-coredata://note/p1" };
    let steps = withCall([], call("ALOUD_NOTES_CREATE", false));
    steps = withResult(steps, { success: true, message: "done", show });
    steps = withCall(steps, call("GMAIL_SEND_EMAIL", false));
    steps = withResult(steps, ok);

    expect(steps[0].result?.show).toEqual(show);
    expect(steps[1].result?.show).toBeNull();
  });

  /**
   * Only the gate event carries the card. Folding the other event of the pair
   * over it must keep it, or the gate falls back to a title and nothing else —
   * the blank approval card the preview exists to end.
   */
  it("the gate keeps its preview when the start event folds in", () => {
    const preview: ActionPreview = {
      kind: "message",
      tier: "confirm",
      verb: "Send",
      title: "Send message to general",
      fields: [],
      more: [],
      set: 2,
      total: 12,
      risks: [],
      pickedBy: null,
      recoverable: null,
      safer: null,
      autoMs: null,
      revision: 0,
      scope: null,
    };
    const gate = { ...call("SLACK_CHAT_POST_MESSAGE", true), preview };
    const start = call("SLACK_CHAT_POST_MESSAGE", false);

    const inOrder = withCall(withCall([], start), gate);
    const reversed = withCall(withCall([], gate), start);

    for (const steps of [inOrder, reversed]) {
      expect(steps).toHaveLength(1);
      expect(steps[0].preview).toEqual(preview);
      expect(pendingStep({ heard: "", steps })?.preview?.title).toBe(
        "Send message to general",
      );
    }
  });

  it("treats a declined step as settled so it cannot re-open the gate", () => {
    let steps = startAndGate([], "GMAIL_SEND_EMAIL");
    steps = steps.map((s) => ({ ...s, confirmed: false }));
    expect(pendingStep({ heard: "", steps })).toBeUndefined();
  });

  /** Rust reports a declined write as a failed result; the user said no. */
  it("reads a declined gate as cancelled, not failed", () => {
    let steps = startAndGate([], "SLACK_SEND_MESSAGE");
    steps = withDecision(steps, false);
    steps = withResult(steps, { success: false, message: "failed", show: null });
    expect(stepStatus(steps[0], false)).toBe("cancelled");
    expect(pendingStep({ heard: "", steps })).toBeUndefined();

    // A real failure after a yes still reads as one.
    let approved = withDecision(startAndGate([], "SLACK_SEND_MESSAGE"), true);
    approved = withResult(approved, { success: false, message: "failed", show: null });
    expect(stepStatus(approved[0], false)).toBe("failed");
  });

  it("carries the gate's Always-allow offer onto the step", () => {
    const steps = withCall(withCall([], call("SLACK_SEND_MESSAGE", false)), {
      ...call("SLACK_SEND_MESSAGE", true),
      canAlways: true,
    });
    expect(steps[0].canAlways).toBe(true);
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
    result: result === undefined ? undefined : { success: result, message: "", show: null },
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
