import { describe, expect, it } from "vitest";
import { canUnzip, driveRoom, driveUsage, isImageName, itemMeta, selectedItem, splitShots } from "./shelf-widgets";

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

describe("file actions", () => {
  it("unzips only real .zip files", () => {
    expect(canUnzip({ kind: "file", name: "Icons.ZIP" })).toBe(true);
    expect(canUnzip({ kind: "folder", name: "backup.zip" })).toBe(false);
    expect(canUnzip({ kind: "file", name: "zip.txt" })).toBe(false);
  });

  it("knows a picture by its extension", () => {
    expect(isImageName("Screen 4.PNG")).toBe(true);
    expect(isImageName("photo.heic")).toBe(true);
    expect(isImageName("notes.png.txt")).toBe(false);
  });

  it("calls a folder a folder and sizes a file", () => {
    expect(itemMeta({ kind: "folder", size: 96 })).toBe("Folder");
    expect(itemMeta({ kind: "file", size: 1_400_000 })).toBe("1.4 MB");
  });
});

describe("selection", () => {
  it("drops a selection whose file left the shelf", () => {
    const items = [{ id: "a" }, { id: "b" }];
    expect(selectedItem(items, "b")).toBe(items[1]);
    expect(selectedItem(items, "gone")).toBeNull();
    expect(selectedItem(items, null)).toBeNull();
  });
});

describe("shots and rooms", () => {
  it("takes the newest as the hero", () => {
    expect(splitShots([1, 2, 3])).toEqual({ latest: 1, rest: [2, 3] });
    expect(splitShots([])).toEqual({ latest: null, rest: [] });
  });

  it("writes a drive's room", () => {
    expect(driveRoom({ size: 16_000_000_000, free: 4_100_000_000 })).toBe("4.1 GB free of 16 GB");
  });
});
