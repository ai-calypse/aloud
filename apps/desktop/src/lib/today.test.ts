import { describe, expect, it } from "vitest";
import { degrees, marketLabel, percent, usesFahrenheit } from "./today";

describe("usesFahrenheit", () => {
  it("goes by region, not language", () => {
    expect(usesFahrenheit("en-US")).toBe(true);
    expect(usesFahrenheit("en-GB")).toBe(false);
    expect(usesFahrenheit("en-CA")).toBe(false);
    expect(usesFahrenheit("fr-FR")).toBe(false);
    // No region to go by, or not a locale at all: Celsius, never a throw.
    expect(usesFahrenheit("en")).toBe(false);
    expect(usesFahrenheit("not a locale")).toBe(false);
  });
});

describe("degrees", () => {
  it("rounds and never shows negative zero", () => {
    expect(degrees(20.5)).toBe("21°");
    expect(degrees(-0.4)).toBe("0°");
    expect(degrees(-3.2)).toBe("-3°");
    expect(degrees(null)).toBe("–");
  });
});

describe("marketLabel", () => {
  // A stale price must never read as live: delayed outranks the market state.
  it("says when a price is not now", () => {
    expect(marketLabel({ state: "open", delayed: false })).toBe("");
    expect(marketLabel({ state: "open", delayed: true })).toBe("delayed");
    expect(marketLabel({ state: "after", delayed: false })).toBe("after hours");
    expect(marketLabel({ state: "closed", delayed: false })).toBe("closed");
    expect(marketLabel({ state: null, delayed: false })).toBe("");
  });
});

describe("percent", () => {
  it("signs a move with a word-shaped sign, and calls a flat day flat", () => {
    expect(percent(0.84)).toBe("+0.8%");
    expect(percent(-1.24)).toBe("−1.2%");
    expect(percent(0.02)).toBe("0.0%");
    expect(percent(-0.04)).toBe("0.0%");
  });
});
