import { describe, expect, it } from "vitest";
import { chunk, runWord, SHORTCUT_ROLES, shortcutRole } from "./launch-logic";

describe("shortcutRole", () => {
  it("gives a shortcut the same colour every time", () => {
    expect(shortcutRole("Block Off an Hour")).toBe(shortcutRole("Block Off an Hour"));
  });

  it("only uses colours that carry white words at AA", () => {
    for (const name of ["a", "Make GIF", "Take a Break", "Email Running Late", "Új parancs", ""]) {
      expect(SHORTCUT_ROLES).toContain(shortcutRole(name));
    }
  });

  it("spreads a library across more than one colour", () => {
    const names = Array.from({ length: 20 }, (_, i) => `Shortcut ${i}`);
    expect(new Set(names.map(shortcutRole)).size).toBeGreaterThan(2);
  });
});

describe("chunk", () => {
  it("splits in order and leaves the last group short", () => {
    expect(chunk([1, 2, 3, 4, 5], 4)).toEqual([[1, 2, 3, 4], [5]]);
    expect(chunk([], 4)).toEqual([]);
  });
});

describe("runWord", () => {
  it("says each state in a word, and nothing before a first run", () => {
    expect(runWord(undefined)).toBeNull();
    expect(runWord({ state: "running" })).toBe("Running…");
    expect(runWord({ state: "done" })).toBe("Done");
    expect(runWord({ state: "failed", message: "x" })).toBe("Failed");
  });
});
