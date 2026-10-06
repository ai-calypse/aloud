import { describe, expect, it } from "vitest";
import { fuzzyScore, rank } from "./fuzzy";

const names = (query: string, from: string[]) => rank(from, query, (n) => n);

describe("fuzzyScore", () => {
  it("returns null when the letters are not there in order", () => {
    expect(fuzzyScore("xyz", "Safari")).toBeNull();
    expect(fuzzyScore("ras", "Safari")).toBeNull();
  });
  it("is case-insensitive", () => {
    expect(fuzzyScore("SAF", "safari")).not.toBeNull();
  });
});

describe("rank", () => {
  it("puts a prefix before a later word before a substring before scattered letters", () => {
    const apps = ["Notes Archive", "Reminders", "Terminal Notes", "Keynote", "Numbers"];
    // Prefix, word start, inside a word, then letters in order.
    expect(names("note", apps)).toEqual(["Notes Archive", "Terminal Notes", "Keynote"]);
    expect(names("nbs", apps)).toEqual(["Numbers"]);
  });
  it("finds an app by its initials", () => {
    expect(names("vsc", ["Visual Studio Code", "Voice Memos", "Safari"])).toEqual(["Visual Studio Code"]);
  });
  it("prefers the tighter of two prefix matches", () => {
    expect(names("pre", ["Preview Pro Edition", "Preview"])).toEqual(["Preview", "Preview Pro Edition"]);
  });
  it("keeps the given order for equal scores and for an empty query", () => {
    expect(names("", ["b", "a"])).toEqual(["b", "a"]);
    expect(names("a", ["Ab", "Ac"])).toEqual(["Ab", "Ac"]);
  });
});
