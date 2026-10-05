import { describe, expect, it } from "vitest";
import type { BtDevice } from "@/lib/bindings";
import {
  batterySummary,
  deviceLevel,
  leastLeft,
  levelAccent,
  levels,
  micWord,
  outputLine,
  rowSubtitle,
} from "./media-format";

const device = (over: Partial<BtDevice> = {}): BtDevice => ({
  name: "AirPods Pro",
  address: "aa:bb:cc:dd:ee:ff",
  kind: "headphones",
  connected: true,
  batteries: [{ label: "", percent: 80 }],
  ...over,
});

describe("outputLine and micWord", () => {
  it("names the transport in lower case, or this Mac when it has none", () => {
    expect(outputLine({ transport: "Bluetooth" })).toBe("Playing through bluetooth");
    expect(outputLine({ transport: "" })).toBe("Playing through this Mac");
    expect(outputLine(undefined)).toBe("Playing through this Mac");
  });

  it("says muted or live", () => {
    expect(micWord(true)).toBe("Muted");
    expect(micWord(false)).toBe("Live");
  });
});

describe("batteries", () => {
  it("judges a device by its lowest battery, and only while it is connected", () => {
    const pods = device({
      batteries: [
        { label: "Left", percent: 100 },
        { label: "Right", percent: 98 },
        { label: "Case", percent: 63 },
      ],
    });
    expect(deviceLevel(pods)).toBe(63);
    expect(deviceLevel(device({ connected: false }))).toBeNull();
    expect(deviceLevel(device({ batteries: [] }))).toBeNull();
  });

  it("summarises like the calm list", () => {
    expect(batterySummary(device({ connected: false }))).toBe("Not connected");
    expect(batterySummary(device({ batteries: [] }))).toBe("Connected");
    expect(batterySummary(device())).toBe("80%");
    expect(
      batterySummary(device({ batteries: [{ label: "Left", percent: 90 }, { label: "Case", percent: 20 }] })),
    ).toBe("Left 90% · Case 20%");
  });

  // One battery's percent sits beside the name; a subtitle would repeat it.
  it("gives a row a subtitle only when the percent beside it cannot say enough", () => {
    expect(rowSubtitle(device())).toBeNull();
    expect(rowSubtitle(device({ connected: false }))).toBe("Not connected");
    expect(rowSubtitle(device({ batteries: [] }))).toBe("Connected");
    expect(
      rowSubtitle(device({ batteries: [{ label: "Left", percent: 90 }, { label: "Case", percent: 20 }] })),
    ).toBe("Left 90% · Case 20%");
  });

  it("is green until it is about to run out, with the same thresholds as Mac Power", () => {
    expect(levelAccent(100)).toBe("ok");
    expect(levelAccent(21)).toBe("ok");
    expect(levelAccent(20)).toBe("orange");
    expect(levelAccent(11)).toBe("orange");
    expect(levelAccent(10)).toBe("move");
    expect(levelAccent(0)).toBe("move");
  });

  it("lists only connected devices with a battery, in order, and finds the least left", () => {
    const all = levels([
      device({ name: "Mouse", kind: "mouse", batteries: [{ label: "", percent: 55 }] }),
      device({ name: "Off", connected: false }),
      device({ name: "Wired", batteries: [] }),
      device({ name: "Pods", batteries: [{ label: "L", percent: 40 }, { label: "R", percent: 41 }] }),
      device({ name: "Keys", kind: "keyboard", batteries: [{ label: "", percent: 40 }] }),
    ]);
    expect(all.map((l) => [l.name, l.percent])).toEqual([["Mouse", 55], ["Pods", 40], ["Keys", 40]]);
    // A tie goes to the one listed first, so the tile does not flicker between two.
    expect(leastLeft(all)?.name).toBe("Pods");
    expect(leastLeft([])).toBeNull();
  });
});
