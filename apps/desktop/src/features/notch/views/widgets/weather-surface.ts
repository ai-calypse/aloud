import type { WidgetRole } from "@/components/notch/widgets";
import type { Sky } from "@/lib/bindings";

/** The ambient picture behind a weather card's words. */
export type Backdrop = "stars" | "rain" | "storm" | "snow" | "clouds" | "fog";

/** What the header draws: a sky's own glyph, or the moon's variants. */
export type Glyph = Sky | "moon" | "cloudMoon";

export type WeatherSurface = {
  /** The card's sky colour, a widget role (tokens.css). */
  role: WidgetRole;
  /** Rain, storm, snow and fog keep their colour and lose the light after dark. */
  dim: boolean;
  backdrop: Backdrop | null;
  glyph: Glyph;
};

/**
 * What a weather card looks like for the condition and the light, the way
 * Apple's Weather widget turns a sky into a card. Pure, so the table of
 * conditions is testable without a render.
 *
 * Clear and partly cloudy by day is the blue card this widget always had.
 * Any night with no precipitation or fog collapses to one navy card: a night
 * overcast and a night clear read the same at a glance, and the glyph says
 * which.
 */
export function weatherSurface(sky: Sky, isDay: boolean): WeatherSurface {
  const night = !isDay;
  switch (sky) {
    case "clear":
      return night
        ? { role: "sky-night", dim: false, backdrop: "stars", glyph: "moon" }
        : { role: "blue", dim: false, backdrop: null, glyph: "clear" };
    case "partlyCloudy":
      return night
        ? { role: "sky-night", dim: false, backdrop: "stars", glyph: "cloudMoon" }
        : { role: "blue", dim: false, backdrop: null, glyph: "partlyCloudy" };
    case "cloudy":
      return night
        ? { role: "sky-night", dim: false, backdrop: "stars", glyph: "cloudy" }
        : { role: "sky-overcast", dim: false, backdrop: "clouds", glyph: "cloudy" };
    case "fog":
      return { role: "sky-fog", dim: night, backdrop: "fog", glyph: "fog" };
    case "drizzle":
    case "rain":
      return { role: "sky-rain", dim: night, backdrop: "rain", glyph: sky };
    case "storm":
      return { role: "sky-storm", dim: night, backdrop: "storm", glyph: "storm" };
    case "snow":
      return { role: "sky-snow", dim: night, backdrop: "snow", glyph: "snow" };
  }
}
