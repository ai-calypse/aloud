import { describe, expect, it } from "vitest";
import type { Battery, SystemStats } from "@/lib/bindings";
import { SNAPS } from "../../snaps";
import { formatRate } from "../mac-format";
import {
  GRID_SNAPS,
  SNAP_REGION,
  WHOLE_SNAPS,
  powerWidget,
  privacyTile,
  rateParts,
  statRings,
  thermalWidget,
  usedFraction,
} from "./mac-widgets";

const stats = (over: Partial<SystemStats> = {}): SystemStats => ({
  cpu: 41,
  memoryUsedGb: 11,
  memoryTotalGb: 16,
  diskUsedGb: 212,
  diskTotalGb: 512,
  netDown: 0,
  netUp: 0,
  thermal: "nominal",
  ...over,
});

describe("statRings", () => {
  it("is CPU, Memory, Disk as Move, Exercise, Stand, outermost first", () => {
    const rings = statRings(stats());
    expect(rings.map((r) => [r.key, r.accent])).toEqual([
      ["cpu", "move"],
      ["memory", "exercise"],
      ["disk", "stand"],
    ]);
    expect(rings[0]).toMatchObject({ value: 0.41, text: "41" });
    expect(rings[1].value).toBeCloseTo(11 / 16);
    expect(rings[1]).toMatchObject({ text: "69", sub: "11.0 GB of 16.0 GB" });
    expect(rings[2]).toMatchObject({ text: "41", sub: "300 GB free" });
  });

  // Rust sends a NaN float as null; a ring of NaN length would draw wrong.
  it("draws an unread value as an empty ring, not a full or broken one", () => {
    const rings = statRings(stats({ cpu: null, memoryTotalGb: null, diskUsedGb: null }));
    expect(rings.map((r) => r.value)).toEqual([0, 0, 0]);
    expect(rings[0].text).toBe("0");
  });

  it("never lets a full disk or a CPU over 100 overfill the ring", () => {
    expect(statRings(stats({ cpu: 140 }))[0].value).toBe(1);
    expect(usedFraction(600, 512)).toBe(1);
  });
});

describe("rateParts", () => {
  it("joins back to formatRate, so the number and its unit never disagree", () => {
    for (const b of [null, 0, 812, 1000, 812_000, 1_400_000, 250_000_000]) {
      const { value, unit } = rateParts(b);
      expect(`${value} ${unit}`).toBe(formatRate(b));
    }
  });
});

describe("thermalWidget", () => {
  it("always has a word, and only the two hot states are urgent", () => {
    expect(thermalWidget("nominal")).toMatchObject({ word: "Normal", accent: "ok", hot: false });
    expect(thermalWidget("fair")).toMatchObject({ word: "Warm", hot: false });
    expect(thermalWidget("serious")).toMatchObject({ word: "Hot", accent: "orange", hot: true });
    expect(thermalWidget("critical")).toMatchObject({ word: "Critical", accent: "move", hot: true });
  });
});

const battery = (over: Partial<Battery> = {}): Battery => ({
  percent: 82,
  state: "discharging",
  minutes: 300,
  cycles: 1204,
  health: 94,
  ...over,
});

describe("powerWidget", () => {
  it("carries the ring, the sentence, cycles and health", () => {
    expect(powerWidget(battery())).toMatchObject({
      value: 0.82,
      text: "82",
      sentence: "5h 00m left",
      cycles: "1,204",
      health: { value: 0.94, text: "94" },
    });
  });

  it("charging says so and says how long", () => {
    const p = powerWidget(battery({ state: "charging", minutes: 45 }));
    expect(p.charging).toBe(true);
    expect(p.sentence).toBe("Charging, 45m to full");
  });

  // A Mac that does not report them gets no tile, not a zero.
  it("leaves out health and cycles a Mac does not report", () => {
    const p = powerWidget(battery({ cycles: null, health: null }));
    expect(p.cycles).toBeNull();
    expect(p.health).toBeNull();
  });
});

describe("privacyTile", () => {
  it("names the mic's apps and says plainly when macOS gives none", () => {
    expect(privacyTile("mic", true, ["Zoom", "Aloud"])).toEqual({
      on: true,
      word: "In use",
      detail: "Zoom, Aloud",
    });
    expect(privacyTile("mic", true, []).detail).toBe("An app is recording");
  });

  it("never invents which app has the camera", () => {
    expect(privacyTile("camera", true, ["Zoom"]).detail).toBe("An app, which macOS does not name");
  });

  it("says Off in words, not only in colour", () => {
    expect(privacyTile("mic", false, [])).toMatchObject({ on: false, word: "Off" });
    expect(privacyTile("camera", false, []).word).toBe("Off");
  });
});

describe("snap glyphs", () => {
  // A new snap with no drawing would be an empty box in the grid.
  it("draws every snap the Windows board offers", () => {
    for (const s of SNAPS) expect(SNAP_REGION[s.id], s.id).toBeTruthy();
  });

  it("offers every snap exactly once between the grid and the whole-window pair", () => {
    const ids = [...GRID_SNAPS, ...WHOLE_SNAPS].map((s) => s.id).sort();
    expect(ids).toEqual(SNAPS.map((s) => s.id).sort());
    expect(GRID_SNAPS).toHaveLength(8);
  });
});
