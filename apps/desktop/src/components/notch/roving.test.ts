import { describe, expect, it } from "vitest";
import { rovingIndex } from "./roving";

describe("rovingIndex", () => {
  it("moves one step and wraps at both ends", () => {
    expect(rovingIndex("ArrowRight", 0, 3)).toBe(1);
    expect(rovingIndex("ArrowRight", 2, 3)).toBe(0);
    expect(rovingIndex("ArrowLeft", 0, 3)).toBe(2);
  });

  it("jumps to the ends", () => {
    expect(rovingIndex("Home", 2, 3)).toBe(0);
    expect(rovingIndex("End", 0, 3)).toBe(2);
  });

  // Tab and the vertical arrows belong to the shell and the page; a strip
  // that swallowed them would trap the keyboard in itself.
  it("leaves every other key alone", () => {
    expect(rovingIndex("Tab", 0, 3)).toBeNull();
    expect(rovingIndex("ArrowDown", 0, 3)).toBeNull();
    expect(rovingIndex("Enter", 0, 3)).toBeNull();
  });

  it("does nothing in an empty strip", () => {
    expect(rovingIndex("ArrowRight", 0, 0)).toBeNull();
  });
});
