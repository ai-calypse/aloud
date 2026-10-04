import { describe, expect, it } from "vitest";
import { ACTIONS } from "../actions";
import { countToday, formatAgo, formatBytes } from "./shelf-data";

describe("formatBytes", () => {
  it("uses Finder's decimal units and drops the decimal once it is two digits", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1000)).toBe("1.0 KB");
    expect(formatBytes(1_400_000)).toBe("1.4 MB");
    expect(formatBytes(42_000_000)).toBe("42 MB");
    expect(formatBytes(906_452_992)).toBe("906 MB");
    expect(formatBytes(2_500_000_000_000)).toBe("2.5 TB");
  });

  // A unit past the table would read "NaN undefined"; it stays in TB.
  it("never runs off the end of its units", () => {
    expect(formatBytes(5e15)).toBe("5000 TB");
  });
});

describe("formatAgo", () => {
  const now = 1_000_000_000_000;
  const at = (secondsAgo: number) => now / 1000 - secondsAgo;
  it("steps through now, minutes, hours and days", () => {
    expect(formatAgo(at(5), now)).toBe("now");
    expect(formatAgo(at(120), now)).toBe("2m");
    expect(formatAgo(at(7200), now)).toBe("2h");
    expect(formatAgo(at(3 * 86400), now)).toBe("3d");
  });

  // A file whose clock is ahead (a network drive) must not read "-3m".
  it("treats a time in the future as now", () => {
    expect(formatAgo(at(-500), now)).toBe("now");
  });
});

describe("countToday", () => {
  it("counts the last 24 hours, not the calendar day", () => {
    const now = 1_000_000_000_000;
    const s = now / 1000;
    expect(countToday([s - 10, s - 86000, s - 87000], now)).toBe(2);
  });
});

describe("Shelf actions", () => {
  // Rust's islands name these in their buttons; a typo is silent until clicked.
  it("registers every action the Shelf's islands send, and the ones that need arguments say so", () => {
    const byId = new Map(ACTIONS.map((a) => [a.id, a]));
    expect(byId.get("shelf.copyShot")?.args).toEqual(["path"]);
    expect(byId.get("shelf.reveal")?.args).toEqual(["path"]);
    expect(byId.get("shelf.eject")?.args).toEqual(["mount"]);
    for (const id of ["shelf.files", "shelf.shots", "shelf.downloads", "shelf.drives", "shelf.capture"]) {
      expect(byId.get(id)?.args).toBeUndefined();
    }
  });
});
