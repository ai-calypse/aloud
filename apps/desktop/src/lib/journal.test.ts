import { describe, expect, it } from "vitest";
import type { Note } from "./bindings";
import { dayLabel, daysBefore, entryPreview, groupByDay, wordCount } from "./journal";

/** Local time, the way the view reads `date`. */
const local = (y: number, m: number, d: number, h = 12) => new Date(y, m, d, h);
const note = (at: Date, content = "a thought"): Note => ({
  path: `/j/${at.getTime()}.md`,
  title: content.split("\n")[0],
  date: Math.floor(at.getTime() / 1000),
  content,
});

describe("dayLabel", () => {
  const now = local(2026, 9, 7, 9);

  it("says today and yesterday by calendar day, not by 24-hour window", () => {
    expect(dayLabel(local(2026, 9, 7, 0), now)).toBe("Today");
    // Late last night is yesterday, though fewer than 24 hours ago.
    expect(dayLabel(local(2026, 9, 6, 23), now)).toBe("Yesterday");
  });

  it("names the weekday within the week, then the date", () => {
    const tuesday = local(2026, 9, 3);
    expect(dayLabel(tuesday, now)).toBe(
      tuesday.toLocaleDateString(undefined, { weekday: "long" }),
    );
    const older = local(2026, 8, 20);
    expect(dayLabel(older, now)).toBe(
      older.toLocaleDateString(undefined, { month: "long", day: "numeric" }),
    );
  });

  it("adds the year only when it is not this one", () => {
    const lastYear = local(2025, 11, 30);
    expect(dayLabel(lastYear, now)).toContain("2025");
  });
});

describe("daysBefore", () => {
  /** The failure guarded: a 23-hour day (clocks forward) floored to zero
   * days, so yesterday's entries read as today's. */
  it("counts calendar days across a short day", () => {
    const now = local(2026, 2, 30, 0);
    expect(daysBefore(local(2026, 2, 29, 0), now)).toBe(1);
    expect(daysBefore(local(2026, 2, 28, 0), now)).toBe(2);
  });
});

describe("groupByDay", () => {
  it("keeps Rust's order and starts a group at each new day", () => {
    const now = local(2026, 9, 7, 18);
    const entries = [
      note(local(2026, 9, 7, 17)),
      note(local(2026, 9, 7, 8)),
      note(local(2026, 9, 6, 22)),
    ];
    const days = groupByDay(entries, now);
    expect(days.map((d) => d.label)).toEqual(["Today", "Yesterday"]);
    expect(days[0].entries).toEqual(entries.slice(0, 2));
    expect(days[1].entries).toEqual([entries[2]]);
  });

  it("is empty for an empty journal", () => {
    expect(groupByDay([])).toEqual([]);
  });
});

describe("entryPreview", () => {
  it("is what follows the first line, on one line", () => {
    expect(entryPreview("Groceries\nmilk\n\n  eggs ")).toBe("milk eggs");
  });

  it("skips leading blank lines and a heading's marks to find the first", () => {
    expect(entryPreview("\n\n# Idea\na podcast about maps")).toBe("a podcast about maps");
  });

  it("is empty when the entry is one line", () => {
    expect(entryPreview("just this")).toBe("");
    expect(entryPreview("")).toBe("");
  });
});

describe("wordCount", () => {
  it("counts words and treats blank as none", () => {
    expect(wordCount("  one two\nthree ")).toBe(3);
    expect(wordCount("   ")).toBe(0);
  });
});
