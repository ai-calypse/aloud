import { describe, expect, it } from "vitest";
import type { NowPlaying } from "@/lib/bindings";
import { formatSpeed, needsSong, formatTime, jsOffFix, joinNames, lineAt, lowest, nextSpeed, plainHost, positionAt, shortName } from "./media";

const track = (over: Partial<NowPlaying> = {}): NowPlaying => ({
  player: "spotify",
  source: { app: "Spotify", bundleId: null, host: null, tab: null },
  state: "playing",
  title: "t",
  artist: "a",
  album: "b",
  durationMs: 200_000,
  positionMs: 10_000,
  speedPct: 100,
  artworkUrl: null,
  song: null,
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

describe("web video speed", () => {
  // Guards: the bar running at 1x under a video playing at 2x, which then
  // jumps back at every poll.
  it("moves the clock at the page's speed", () => {
    expect(positionAt(track({ speedPct: 200 }), 1000, 3500)).toBe(15_000);
    expect(positionAt(track({ speedPct: 75 }), 0, 4000)).toBe(13_000);
  });

  it("steps through the speeds and comes back to normal", () => {
    expect([100, 125, 150, 175, 200, 75].map(nextSpeed)).toEqual([125, 150, 175, 200, 75, 100]);
    // A speed the page chose itself (YouTube offers 0.25) is not on the list.
    expect(nextSpeed(25)).toBe(100);
  });

  it("writes speeds the way people say them", () => {
    expect(formatSpeed(100)).toBe("1×");
    expect(formatSpeed(125)).toBe("1.25×");
    expect(formatSpeed(75)).toBe("0.75×");
    expect(formatSpeed(150)).toBe("1.5×");
  });
});

describe("browser wording", () => {
  it("names the browser the way its user does", () => {
    expect(shortName("Brave Browser")).toBe("Brave");
    expect(shortName("Google Chrome")).toBe("Chrome");
    expect(shortName("Microsoft Edge")).toBe("Edge");
    expect(shortName("Safari")).toBe("Safari");
    expect(plainHost("www.youtube.com")).toBe("youtube.com");
    expect(plainHost(null)).toBe("");
  });

  it("points at the right menu", () => {
    expect(jsOffFix("Brave Browser")).toBe("Brave › View › Developer › Allow JavaScript from Apple Events.");
    expect(jsOffFix("Safari")).toContain("Develop");
    expect(jsOffFix("Safari")).toContain("Show features for web developers");
  });

  it("lists browsers in a sentence", () => {
    const issue = (app: string) => ({ app, kind: "jsOff" as const });
    expect(joinNames([issue("Brave Browser")])).toBe("Brave");
    expect(joinNames([issue("Brave Browser"), issue("Safari")])).toBe("Brave and Safari");
    expect(joinNames([issue("Brave Browser"), issue("Google Chrome"), issue("Safari")])).toBe("Brave, Chrome and Safari");
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

describe("needsSong", () => {
  // Guards: lyrics for a browser title or a podcast episode being looked up as
  // if it were a song; only Spotify and Music name their own tracks.
  it("is true for everything but the two players that name their tracks", () => {
    expect(needsSong(track({ player: "spotify" }))).toBe(false);
    expect(needsSong(track({ player: "music" }))).toBe(false);
    expect(needsSong(track({ player: "browser" }))).toBe(true);
    expect(needsSong(track({ player: "mediaRemote" }))).toBe(true);
  });
});
