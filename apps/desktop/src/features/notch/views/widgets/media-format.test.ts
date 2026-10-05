import { describe, expect, it } from "vitest";
import type { BtDevice } from "@/lib/bindings";
import {
  deviceLevel,
  levelAccent,
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
  level: 80,
  case: null,
  singleBud: null,
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
  it("headlines the level Rust worked out, and only while connected", () => {
    expect(deviceLevel(device({ level: 99, case: 63 }))).toBe(99);
    expect(deviceLevel(device({ connected: false }))).toBeNull();
    expect(deviceLevel(device({ level: null, batteries: [] }))).toBeNull();
  });

  // Guards: the card headlined the case (63%) for buds at 100% and 98%.
  it("puts the case in the small print, never the headline", () => {
    const pods = device({ level: 99, case: 63 });
    expect(deviceLevel(pods)).toBe(99);
    expect(rowSubtitle(pods)).toBe("Case 63%");
  });

  it("says when one bud is the whole figure", () => {
    expect(rowSubtitle(device({ level: 41, case: 90, singleBud: "Left" }))).toBe("Left bud only · Case 90%");
    expect(rowSubtitle(device({ level: 41, singleBud: "Right" }))).toBe("Right bud only");
  });

  it("is quiet when the percent beside the name says it all, and honest when there is none", () => {
    expect(rowSubtitle(device())).toBeNull();
    expect(rowSubtitle(device({ connected: false }))).toBe("Not connected");
    expect(rowSubtitle(device({ level: null, batteries: [] }))).toBe("Connected");
    expect(rowSubtitle(device({ level: null, case: 55 }))).toBe("Case 55%");
  });

  it("is green until it is about to run out, with the same thresholds as Mac Power", () => {
    expect(levelAccent(100)).toBe("ok");
    expect(levelAccent(21)).toBe("ok");
    expect(levelAccent(20)).toBe("orange");
    expect(levelAccent(11)).toBe("orange");
    expect(levelAccent(10)).toBe("move");
    expect(levelAccent(0)).toBe("move");
  });
});
