import { describe, expect, it } from "vitest";
import { SECTIONS } from "./registry";
import { viewFor } from "./sections";

describe("registry widget views and their calm fallback", () => {
  // A widget view may never be the only view: the calm one is what the other look and every
  // undrawn state fall back to.
  it("has a calm view behind every widget view", () => {
    for (const section of SECTIONS) {
      for (const sub of section.subsections) {
        if (sub.widgetView) expect(sub.view, `${section.id}/${sub.id}`).toBeDefined();
      }
    }
  });

  it("draws every Today subsection as widgets, and each still has its calm view", () => {
    const today = SECTIONS.find((s) => s.id === "today")!;
    for (const id of ["calendar", "reminders", "weather", "stocks"]) {
      const sub = today.subsections.find((s) => s.id === id)!;
      expect(viewFor(sub, "widgets"), id).toBe(sub.widgetView);
      expect(sub.widgetView, id).toBeDefined();
      expect(viewFor(sub, "calm"), id).toBe(sub.view);
    }
  });
});
