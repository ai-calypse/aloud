import { describe, expect, it } from "vitest";
import type { Sky } from "@/lib/bindings";
import { weatherSurface } from "./weather-surface";

const SKIES: Sky[] = ["clear", "partlyCloudy", "cloudy", "fog", "drizzle", "rain", "snow", "storm"];

describe("weatherSurface", () => {
  it("keeps the blue card for a clear or partly cloudy day", () => {
    expect(weatherSurface("clear", true)).toMatchObject({ role: "blue", backdrop: null, glyph: "clear" });
    expect(weatherSurface("partlyCloudy", true)).toMatchObject({ role: "blue", glyph: "partlyCloudy" });
  });

  it("turns a dry night into the navy card with stars and the moon's glyphs", () => {
    expect(weatherSurface("clear", false)).toEqual({ role: "sky-night", dim: false, backdrop: "stars", glyph: "moon" });
    expect(weatherSurface("partlyCloudy", false).glyph).toBe("cloudMoon");
    // The glyph, not the card, says a night is overcast.
    expect(weatherSurface("cloudy", false)).toMatchObject({ role: "sky-night", glyph: "cloudy" });
  });

  it("gives precipitation and fog their own card, darkened rather than replaced at night", () => {
    for (const [sky, role] of [
      ["rain", "sky-rain"],
      ["drizzle", "sky-rain"],
      ["storm", "sky-storm"],
      ["snow", "sky-snow"],
      ["fog", "sky-fog"],
    ] as const) {
      expect(weatherSurface(sky, true)).toMatchObject({ role, dim: false });
      expect(weatherSurface(sky, false)).toMatchObject({ role, dim: true });
    }
  });

  /** The failure: a rainy night drawn as the clear-night card, hiding the rain. */
  it("never lets the night card swallow weather that is falling", () => {
    for (const sky of ["drizzle", "rain", "storm", "snow"] as const) {
      expect(weatherSurface(sky, false).role).not.toBe("sky-night");
      expect(weatherSurface(sky, false).backdrop).not.toBe("stars");
    }
  });

  it("answers every sky, day and night, with a backdrop only where there is something to draw", () => {
    for (const sky of SKIES) {
      for (const isDay of [true, false]) {
        const s = weatherSurface(sky, isDay);
        expect(s.role).toBeTruthy();
        if (sky === "clear" && isDay) expect(s.backdrop).toBeNull();
      }
    }
  });
});
