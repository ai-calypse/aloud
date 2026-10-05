import { describe, expect, it } from "vitest";
import { READY, SECTIONS } from "./registry";
import { viewFor } from "./sections";

describe("registry widget views", () => {
  it("gives Now › Overview a widget view, so Home is a board in the widgets look", () => {
    const overview = READY.find((s) => s.id === "now")?.subsections.find((s) => s.id === "overview");
    expect(overview?.widgetView).toBeDefined();
    expect(viewFor(overview!, "widgets")).toBe(overview!.widgetView);
    expect(viewFor(overview!, "calm")).toBe(overview!.view);
  });

  // The look must never blank a subsection nobody has designed as widgets.
  it("shows every shipped subsection in either look", () => {
    for (const section of READY) {
      for (const sub of section.subsections) {
        expect(viewFor(sub, "widgets"), `${section.id}/${sub.id}`).toBeDefined();
        expect(viewFor(sub, "calm"), `${section.id}/${sub.id}`).toBe(sub.view);
      }
    }
  });

  it("never lists a widget view on a subsection that is not ready", () => {
    for (const section of SECTIONS) {
      for (const sub of section.subsections) {
        if (sub.widgetView) expect(sub.ready, `${section.id}/${sub.id}`).toBe(true);
      }
    }
  });

  // `readySections` drops a subsection with no calm view, so a widget view on
  // one would be unreachable, and `viewFor` would have nothing to fall back to.
  it("gives every widget view a calm view to fall back to", () => {
    for (const section of SECTIONS) {
      for (const sub of section.subsections) {
        if (sub.widgetView) expect(sub.view, `${section.id}/${sub.id}`).toBeDefined();
      }
    }
  });

  it("draws Agents and Launch as widgets, leaving Launch › Search calm", () => {
    const sub = (s: string, id: string) =>
      READY.find((x) => x.id === s)!.subsections.find((x) => x.id === id)!;
    for (const [s, id] of [
      ["agents", "voice"],
      ["agents", "coding"],
      ["agents", "usage"],
      ["launch", "apps"],
      ["launch", "shortcuts"],
    ]) {
      expect(sub(s, id).widgetView, `${s}/${id}`).toBeDefined();
    }
    expect(sub("launch", "search").widgetView).toBeUndefined();
  });
});
