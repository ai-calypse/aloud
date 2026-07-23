import { describe, expect, it } from "vitest";
import { diffLines, diffStat } from "./diff";

describe("diffLines", () => {
  /** The common case: a few lines change in the middle of a block. */
  it("trims the shared head and tail down to the change", () => {
    const before = "a\nb\nc\nd\ne";
    const after = "a\nb\nC!\nd\ne";
    expect(diffLines(before, after)).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "del", text: "c" },
      { kind: "add", text: "C!" },
      { kind: "same", text: "d" },
      { kind: "same", text: "e" },
    ]);
  });

  /** A Write has no before — everything is an addition. */
  it("renders pure insertion when there is no before", () => {
    const lines = diffLines("", "one\ntwo");
    expect(lines).toEqual([
      { kind: "add", text: "one" },
      { kind: "add", text: "two" },
    ]);
    expect(diffStat(lines)).toBe("+2");
  });

  /** The bug this catches: a line equal at both ends must not be claimed by
      both the prefix and the suffix, doubling it. */
  it("does not double-count a line shared by head and tail", () => {
    const lines = diffLines("x", "x\ny\nx");
    const added = lines.filter((l) => l.kind === "add").map((l) => l.text);
    expect(added).toEqual(["y", "x"]);
    expect(lines.filter((l) => l.kind === "del")).toEqual([]);
  });

  it("identical input is all context and no change", () => {
    const lines = diffLines("a\nb", "a\nb");
    expect(lines.every((l) => l.kind === "same")).toBe(true);
    expect(diffStat(lines)).toBe("no change");
  });

  /** Context is capped so a one-line edit in a long file stays a short card. */
  it("keeps at most two context lines each side", () => {
    const before = ["1", "2", "3", "4", "5", "X", "6", "7", "8", "9"].join("\n");
    const after = ["1", "2", "3", "4", "5", "Y", "6", "7", "8", "9"].join("\n");
    const lines = diffLines(before, after);
    expect(lines.map((l) => l.text)).toEqual(["4", "5", "X", "Y", "6", "7"]);
  });
});
