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

  // viewFor falls back to the calm view, and readySections drops a subsection
  // with none: a widget view without one would vanish from the panel.
  it("only registers a widget view beside a calm view", () => {
    for (const section of SECTIONS) {
      for (const sub of section.subsections) {
        if (sub.widgetView) expect(sub.view, `${section.id}/${sub.id}`).toBeDefined();
      }
    }
  });

  it("draws every Shelf subsection as widgets", () => {
    const shelf = READY.find((s) => s.id === "shelf");
    for (const sub of shelf?.subsections ?? []) expect(sub.widgetView, `shelf/${sub.id}`).toBeDefined();
  });
});
