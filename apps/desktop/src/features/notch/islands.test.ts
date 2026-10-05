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
  pillSides,
  PILL_MAX,
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

describe("pillSides", () => {
  const ids = (list: Island[]) => list.map((i) => i.id);

  it("spans one activity across both sides of the camera", () => {
    const sides = pillSides([island("a")]);
    expect([ids(sides.left), ids(sides.right), sides.more]).toEqual([["a"], [], 0]);
  });

  it("alternates by rank, so the top two sit either side of the camera", () => {
    const sides = pillSides(["a", "b", "c"].map((id) => island(id)));
    expect([ids(sides.left), ids(sides.right)]).toEqual([["a", "c"], ["b"]]);
  });

  it("counts what does not fit instead of crowding the notch", () => {
    const sides = pillSides(["a", "b", "c", "d", "e", "f"].map((id) => island(id)));
    expect(ids(sides.left).length + ids(sides.right).length).toBe(PILL_MAX);
    expect(sides.more).toBe(2);
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

  it("speaks red for what is critical and green for a write that landed", () => {
    expect(islandTone(alert({ id: "mac:battery-low" }))).toBe("danger");
    expect(islandTone(alert({ id: "x", priority: "urgent" }))).toBe("danger");
    expect(islandTone(alert({ id: "toast:3" }))).toBe("ok");
    expect(islandTone(alert({ id: "shot:/tmp/a.png" }))).toBeUndefined();
  });
});
