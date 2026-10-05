import { describe, expect, it } from "vitest";
import { ago, filterSnippets, hexToRgb } from "./clipboard";

describe("ago", () => {
  it("reads the unit a glance needs", () => {
    expect(ago(100, 130)).toBe("now");
    expect(ago(0, 300)).toBe("5m");
    expect(ago(0, 7200)).toBe("2h");
    expect(ago(0, 86_400 * 3)).toBe("3d");
  });
  it("never goes negative when the clock runs behind the entry", () => {
    expect(ago(1000, 10)).toBe("now");
  });
});

describe("hexToRgb", () => {
  it("converts, and hands back what it cannot read", () => {
    expect(hexToRgb("#ff8800")).toBe("rgb(255, 136, 0)");
    expect(hexToRgb("nope")).toBe("nope");
  });
});

describe("filterSnippets", () => {
  const all = [
    { name: "Signature", trigger: "sig", body: "Best,\nYaksh" },
    { name: "Address", trigger: "addr", body: "1 Main St" },
  ];
  it("needs every word, in any field", () => {
    expect(filterSnippets(all, "sig best")).toEqual([all[0]]);
    expect(filterSnippets(all, "main")).toEqual([all[1]]);
    expect(filterSnippets(all, "sig main")).toEqual([]);
  });
  it("shows everything for an empty query", () => {
    expect(filterSnippets(all, "  ")).toEqual(all);
  });
});
