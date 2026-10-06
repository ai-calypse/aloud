import { describe, expect, it } from "vitest";
import { TAKEOVER_PREVIEWS } from "./previews";
import { isNudge, nudgeRole } from "./Nudge";
import { toastForm } from "./NotchToast";

describe("nudges", () => {
  it("accents each app as the design colours it, and an unknown app in the agent's indigo", () => {
    const role = (k: keyof typeof TAKEOVER_PREVIEWS) => nudgeRole(TAKEOVER_PREVIEWS[k]);
    expect(role("nudgeMsg")).toBe("ok");
    expect(role("nudgeMail")).toBe("info");
    expect(role("nudgeSlack")).toBe("podcast");
    expect(role("nudgeCal")).toBe("calendar");
    expect(role("nudgeRem")).toBe("orange");
    expect(role("nudgeCall")).toBe("ok");
    expect(role("nudgeGeneric")).toBe("agent");
  });

  it("recognises only what Rust names nudge, calendar-soon and reminder-due", () => {
    expect(isNudge(TAKEOVER_PREVIEWS.nudgeCal)).toBe(true);
    expect(isNudge(TAKEOVER_PREVIEWS.shot)).toBe(false);
    expect(isNudge(TAKEOVER_PREVIEWS.volume)).toBe(false);
  });

  it("draws a call as a card, a message as a peek, and a message while typing as a capsule until pointed at", () => {
    expect(toastForm(TAKEOVER_PREVIEWS.nudgeCall, false)).toBe("card");
    expect(toastForm(TAKEOVER_PREVIEWS.nudgeMsg, false)).toBe("peek");
    expect(toastForm(TAKEOVER_PREVIEWS.nudgeMsgTyping, false)).toBe("capsule");
    expect(toastForm(TAKEOVER_PREVIEWS.nudgeMsgTyping, true)).toBe("peek");
  });

  it("offers only buttons Aloud can honour: Open and Join, never Accept or Decline", () => {
    const labels = Object.entries(TAKEOVER_PREVIEWS)
      .filter(([, island]) => isNudge(island))
      .flatMap(([, island]) => island.actions.map((a) => a.label));
    expect(new Set(labels)).toEqual(new Set(["Open", "Join"]));
  });
});
