import { describe, expect, it } from "vitest";
import type { Island } from "@/lib/bindings";
import { applyIsland, pillSides, PILL_MAX } from "./islands";

const island = (id: string, detail = ""): Island => ({
  id,
  kind: "live",
  priority: "normal",
  icon: null,
  title: id,
  detail,
  actions: [],
  expires: null,
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
