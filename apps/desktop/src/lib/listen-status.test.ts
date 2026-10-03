import { describe, expect, it } from "vitest";
import type { ListenState } from "./bindings";
import { barStatus, stepFor } from "./listen-status";

const idle: ListenState = { mic: true, transcript: "", busy: false, queued: 0, problem: null };

describe("barStatus", () => {
  it("shows the step while working, and how it ended once idle", () => {
    expect(barStatus({ ...idle, busy: true }, "")).toBe("Working…");
    expect(barStatus({ ...idle, busy: true }, "Click All Clear")).toBe("Click All Clear");
    expect(barStatus(idle, "97")).toBe("97 · Listening");
    expect(barStatus({ ...idle, mic: false }, "")).toBe("Mic off");
  });

  /** A dropped stream must not hide behind a stale "Listening". */
  it("puts a problem above everything", () => {
    expect(barStatus({ ...idle, busy: true, problem: "Reconnecting" }, "Click 9")).toBe("Reconnecting");
  });
});

describe("stepFor", () => {
  it("clears on a new run and keeps the first line of an ending", () => {
    expect(stepFor({ kind: "runStarted", value: "open calculator" })).toBe("");
    expect(stepFor({ kind: "runFinished", value: "97\nfrom Calculator" })).toBe("97");
    expect(stepFor({ kind: "runFinished", value: "" })).toBe("Done");
  });
});
