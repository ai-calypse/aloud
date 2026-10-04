import { describe, expect, it } from "vitest";
import {
  batteryTone,
  chargeSentence,
  formatMinutes,
  formatRate,
  num,
  privacyLead,
  usageTone,
} from "./mac-format";

describe("mac formatting", () => {
  // Rust sends a NaN float as null; "NaN%" or an empty bar's width of NaN
  // would be the quiet failure.
  it("reads a null float as zero", () => {
    expect(num(null)).toBe(0);
    expect(num(Number.NaN)).toBe(0);
    expect(num(12.5)).toBe(12.5);
    expect(formatRate(num(null))).toBe("0 B/s");
  });

  it("scales a rate to the unit a person reads", () => {
    expect(formatRate(812)).toBe("812 B/s");
    expect(formatRate(12_400)).toBe("12 KB/s");
    expect(formatRate(1_400_000)).toBe("1.4 MB/s");
    expect(formatRate(250_000_000)).toBe("250 MB/s");
  });

  it("pads minutes so 7h 05m does not read as 7h 5m", () => {
    expect(formatMinutes(45)).toBe("45m");
    expect(formatMinutes(425)).toBe("7h 05m");
    expect(formatMinutes(60)).toBe("1h 00m");
  });

  it("says what the battery is doing, with no time while macOS estimates", () => {
    expect(chargeSentence("discharging", 440)).toBe("7h 20m left");
    expect(chargeSentence("discharging", null)).toMatch(/estimating/);
    expect(chargeSentence("charging", 45)).toBe("Charging, 45m to full");
    expect(chargeSentence("full", null)).toBe("Fully charged");
  });

  it("goes amber and red only while the battery is draining", () => {
    expect(batteryTone(15, "discharging")).toBe("warn");
    expect(batteryTone(8, "discharging")).toBe("danger");
    expect(batteryTone(8, "charging")).toBe("ok");
    expect(batteryTone(60, "discharging")).toBe("ink");
  });

  it("tones a usage meter at 70 and 90", () => {
    expect([usageTone(40), usageTone(70), usageTone(95)]).toEqual(["ink", "warn", "danger"]);
  });

  it("leads with what is on", () => {
    expect(privacyLead(false, false)).toBe("All quiet");
    expect(privacyLead(true, true)).toBe("Mic and camera on");
  });
});
