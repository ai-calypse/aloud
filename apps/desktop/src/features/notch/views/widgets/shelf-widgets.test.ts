import { describe, expect, it } from "vitest";
import { driveRoom, driveUsage, isImageName } from "./shelf-widgets";

describe("driveUsage", () => {
  it("is green with room, amber as it fills, red when nearly full, each with its word", () => {
    expect(driveUsage({ size: 1000, free: 600 })).toMatchObject({ value: 0.4, percent: 40, word: "", accent: "ok" });
    expect(driveUsage({ size: 1000, free: 150 })).toMatchObject({ percent: 85, word: "Getting full", accent: "orange" });
    expect(driveUsage({ size: 1000, free: 20 })).toMatchObject({ percent: 98, word: "Almost full", accent: "move" });
  });

  // A read that says free > size must not draw an arc past the ring.
  it("keeps the arc inside the ring", () => {
    expect(driveUsage({ size: 1000, free: 2000 }).value).toBe(0);
    expect(driveUsage({ size: 1000, free: -5 }).value).toBe(1);
  });

  it("has no percentage for a drive that reports no size", () => {
    expect(driveUsage({ size: 0, free: 0 })).toEqual({ value: 0, percent: null, word: "", accent: "ok" });
  });
});

describe("isImageName", () => {
  it("knows a picture by its extension", () => {
    expect(isImageName("Screen 4.PNG")).toBe(true);
    expect(isImageName("photo.heic")).toBe(true);
    expect(isImageName("notes.png.txt")).toBe(false);
  });
});

describe("driveRoom", () => {
  it("writes a drive's room", () => {
    expect(driveRoom({ size: 16_000_000_000, free: 4_100_000_000 })).toBe("4.1 GB free of 16 GB");
  });
});
