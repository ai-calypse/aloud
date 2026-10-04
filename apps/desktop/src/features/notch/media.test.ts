import { describe, expect, it } from "vitest";
import type { NowPlaying } from "@/lib/bindings";
import { formatTime, lineAt, lowest, positionAt } from "./media";

const track = (over: Partial<NowPlaying> = {}): NowPlaying => ({
  player: "spotify",
  state: "playing",
  title: "t",
  artist: "a",
  album: "b",
  durationMs: 200_000,
  positionMs: 10_000,
  artworkUrl: null,
  ...over,
});

describe("lineAt", () => {
  const lines = [{ ms: 1000 }, { ms: 2000 }, { ms: 2000 }, { ms: 5000 }];

  // Mirrors the lyrics parser's guarantee: a position exactly on a stamp is
  // already that line, and before the first stamp nothing is highlighted.
  it("is the last line that has started", () => {
    expect(lineAt(lines, 0)).toBe(-1);
    expect(lineAt(lines, 999)).toBe(-1);
    expect(lineAt(lines, 1000)).toBe(0);
    expect(lineAt(lines, 1999)).toBe(0);
    expect(lineAt(lines, 2000)).toBe(2);
    expect(lineAt(lines, 999_999)).toBe(3);
    expect(lineAt([], 5)).toBe(-1);
  });
});

describe("positionAt", () => {
  it("runs the clock between polls while playing", () => {
    expect(positionAt(track(), 1000, 3500)).toBe(12_500);
  });

  // Guards: a paused track's bar creeping on between polls.
  it("holds still while paused", () => {
    expect(positionAt(track({ state: "paused" }), 1000, 9000)).toBe(10_000);
  });

  it("never runs past the end of the track or backwards", () => {
    expect(positionAt(track({ positionMs: 199_000 }), 0, 60_000)).toBe(200_000);
    expect(positionAt(track(), 5000, 1000)).toBe(10_000);
    expect(positionAt(track({ durationMs: 0 }), 0, 5000)).toBe(15_000);
  });
});

describe("formatTime", () => {
  it("reads as m:ss, and h:mm:ss past an hour", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(65_999)).toBe("1:05");
    expect(formatTime(3_600_000)).toBe("1:00:00");
    expect(formatTime(-5)).toBe("0:00");
    expect(formatTime(Number.NaN)).toBe("0:00");
  });
});

describe("lowest", () => {
  it("is the battery that runs out first", () => {
    expect(lowest([{ percent: 100 }, { percent: 18 }, { percent: 63 }])).toBe(18);
    expect(lowest([])).toBeNull();
  });
});
