import { describe, expect, it } from "vitest";
import type { Island } from "@/lib/bindings";
import {
  applyIsland,
  capsuleText,
  dwellMs,
  isCapsule,
  islandRole,
  islandTone,
  levelOf,
  opensOnHover,
  pillLayout,
  pillSideWidths,
  PILL_SIDE_MAX,
  STACK_MAX,
  takeoverTier,
} from "./islands";

const island = (id: string, detail = ""): Island => ({
  id,
  kind: "live",
  priority: "normal",
  icon: null,
  title: id,
  detail,
  actions: [],
  expires: null,
  dismissible: false,
});

describe("applyIsland", () => {
  it("adds, updates in place, and removes", () => {
    let list = applyIsland([], { change: { op: "add", value: island("a") }, order: ["a"] });
    list = applyIsland(list, { change: { op: "add", value: island("b") }, order: ["a", "b"] });
    list = applyIsland(list, {
      change: { op: "update", value: island("a", "18:41") },
      order: ["a", "b"],
    });
    expect(list.map((i) => [i.id, i.detail])).toEqual([
      ["a", "18:41"],
      ["b", ""],
    ]);
    list = applyIsland(list, { change: { op: "remove", value: "a" }, order: ["b"] });
    expect(list.map((i) => i.id)).toEqual(["b"]);
  });

  // The ranking is Rust's alone: the webview follows `order`, so a raised
  // priority moves an island without a second ranking here to disagree.
  it("orders by the ranking Rust sent, not by arrival", () => {
    const list = applyIsland([island("download")], {
      change: { op: "add", value: island("agents") },
      order: ["agents", "download"],
    });
    expect(list.map((i) => i.id)).toEqual(["agents", "download"]);
  });

  it("keeps an island the order does not name, last, rather than dropping it", () => {
    const list = applyIsland([island("a")], {
      change: { op: "add", value: island("b") },
      order: ["b"],
    });
    expect(list.map((i) => i.id)).toEqual(["b", "a"]);
  });
});

describe("pillLayout", () => {
  const ids = (list: Island[]) => list.map((i) => i.id);
  const many = (n: number) => Array.from({ length: n }, (_, i) => island(`i${i}`, `${i}`));

  it("draws nothing for nothing, and one activity alone", () => {
    expect(pillLayout([])).toEqual({ lead: null, stack: [], more: 0 });
    const one = pillLayout(many(1));
    expect([one.lead?.id, one.stack, one.more]).toEqual(["i0", [], 0]);
  });

  it("leads with the most important and stacks the next few behind it, in rank", () => {
    const { lead, stack, more } = pillLayout(many(3));
    expect([lead?.id, ids(stack), more]).toEqual(["i0", ["i1", "i2"], 0]);
  });

  it("caps the stack and counts the rest instead of crowding the notch", () => {
    const { stack, more } = pillLayout(many(12));
    expect(stack).toHaveLength(STACK_MAX);
    expect(more).toBe(12 - 1 - STACK_MAX);
  });

  // Guards: the pill used to add an island a side per activity and grew with them.
  it("is no wider for twelve activities than for five, and none outgrows the bound", () => {
    const widest = (n: number) => {
      const { left, right } = pillSideWidths(many(n));
      return Math.max(left, right);
    };
    for (const n of [1, 2, 5, 12]) expect(widest(n)).toBeLessThanOrEqual(PILL_SIDE_MAX);
    expect(widest(12)).toBe(widest(5));
    expect(widest(5)).toBeLessThanOrEqual(PILL_SIDE_MAX);
  });
});

describe("islandRole", () => {
  it("takes the owner's accent from the id's prefix", () => {
    expect(islandRole(island("agents"))).toBe("agent");
    expect(islandRole(island("media:browser:Brave:youtube.com"))).toBe("media");
    expect(islandRole(island("timer"))).toBe("move");
    expect(islandRole(island("calendar:next"))).toBe("calendar");
  });

  it("leaves an owner with no role, and an urgent island, in the shared ink", () => {
    expect(islandRole(island("toast:3"))).toBeUndefined();
    expect(islandRole({ ...island("battery"), priority: "urgent" })).toBeUndefined();
  });

  // Each event takes the
  // colour docs/design/notch-events.html gives it.
  it("gives every system event the design's colour", () => {
    const ev = (id: string, patch: Partial<Island> = {}) => islandRole({ ...island(id), ...patch });
    expect(ev("hud:volume")).toBe("orange");
    expect(ev("hud:brightness")).toBe("sun");
    expect(ev("hud:caps")).toBe("info");
    expect(ev("bt:70:ae")).toBe("info");
    expect(ev("bt:low:70:ae")).toBe("sun");
    expect(ev("net:wifi")).toBe("cyan");
    expect(ev("drive:/Volumes/X")).toBe("cyan");
    expect(ev("power:charger", { icon: "bolt" })).toBe("ok");
    expect(ev("power:charger", { icon: "battery" })).toBeUndefined();
    expect(ev("power:lowpower", { short: "Low Power" })).toBe("sun");
    expect(ev("mac:privacy-seen", { icon: "camera" })).toBe("ok");
    expect(ev("mac:privacy-seen", { icon: "mic" })).toBe("orange");
  });

  it("speaks a state's colour: a muted mic is red, a live one green", () => {
    expect(islandRole({ ...island("hud:mic"), icon: "mic-off" })).toBe("alert");
    expect(islandRole({ ...island("hud:mic"), icon: "mic" })).toBe("ok");
  });

  // `mac:battery-full` shares the low battery's prefix and was drawn red.
  it("draws a full battery green, not in the low battery's red", () => {
    expect(islandRole(island("mac:battery-full"))).toBe("ok");
    expect(islandRole(island("mac:battery-low"))).toBe("alert");
    expect(islandTone(island("mac:battery-full"))).toBeUndefined();
  });
});

describe("takeovers", () => {
  const alert = (patch: Partial<Island> = {}): Island => ({
    ...island("toast:1", "Sent"),
    kind: "alert",
    ...patch,
  });

  // Every push made before tiers must keep working: an alert is a peek, a
  // live island is not a takeover at all.
  it("maps an island with no tier by its kind", () => {
    expect(takeoverTier(alert())).toBe("peek");
    expect(takeoverTier(island("agents"))).toBeNull();
    expect(takeoverTier(alert({ tier: "whisper" }))).toBe("whisper");
  });

  it("draws a whisper, or a shrunk peek, as a capsule — never a bigger one", () => {
    expect(isCapsule(alert({ tier: "whisper" }))).toBe(true);
    expect(isCapsule(alert({ tier: "peek", shrunk: true }))).toBe(true);
    expect(isCapsule(alert({ tier: "card" }))).toBe(false);
    expect(isCapsule(alert())).toBe(false);
  });

  it("opens a capsule on hover only when there is something behind it", () => {
    expect(opensOnHover(alert({ tier: "whisper" }))).toBe(false);
    const undo = { label: "Undo", action: "files.undoTidy", args: {} };
    expect(opensOnHover(alert({ tier: "whisper", actions: [undo] }))).toBe(true);
    expect(opensOnHover(alert({ tier: "peek", shrunk: true }))).toBe(true);
  });

  it("dwells 1.8 s as a whisper, 4 s as a peek, never as a card", () => {
    expect(dwellMs(alert({ tier: "whisper" }))).toBe(1800);
    expect(dwellMs(alert())).toBe(4000);
    expect(dwellMs(alert({ tier: "card" }))).toBeNull();
    expect(dwellMs(alert({ tier: "whisper", dwellMs: 1500 }))).toBe(1500);
  });

  // The bar is drawn from this: a volume key repeating past the end must not
  // push the fill outside its track.
  it("keeps a level inside the bar", () => {
    expect(levelOf(alert({ level: 140 }))).toBe(100);
    expect(levelOf(alert({ level: -3 }))).toBe(0);
    expect(levelOf(alert())).toBeNull();
  });

  it("says its one word, else its level, else its title", () => {
    expect(capsuleText(alert({ short: "20%", level: 20 }))).toBe("20%");
    expect(capsuleText(alert({ level: 64.4 }))).toBe("64");
    expect(capsuleText(alert({ title: "Muted" }))).toBe("Muted");
  });

  // The tone is only for what the word alone cannot say; a 20% battery is red
  // through its role (islandRole).
  it("speaks red for what is critical and green for a write that landed", () => {
    expect(islandTone(alert({ id: "mac:battery-low" }))).toBeUndefined();
    expect(islandTone(alert({ id: "x", priority: "urgent" }))).toBe("danger");
    expect(islandTone(alert({ id: "toast:3" }))).toBe("ok");
    expect(islandTone(alert({ id: "shot:/tmp/a.png" }))).toBeUndefined();
  });
});
