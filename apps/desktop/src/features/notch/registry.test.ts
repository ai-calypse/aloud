import { describe, expect, it } from "vitest";
import { READY, SECTIONS } from "./registry";
import { CalmHome } from "./views/CalmHome";
import { LaunchSearch } from "./views/Launch";

describe("registry Home", () => {
  it("calls the first section Home and draws its Overview as the Calm tile board", () => {
    const home = READY[0];
    expect(home.id).toBe("now");
    expect(home.name).toBe("Home");
    expect(home.subsections.find((s) => s.id === "overview")?.view).toBe(CalmHome);
  });

  it("holds Home to one subsection, so the chrome hides the switcher", () => {
    expect(READY[0].subsections).toHaveLength(1);
  });

  it("gives every section a footer caption, in the prototype's words for Home", () => {
    expect(READY[0].blurb).toBe("ranked by what is live, biggest first");
    for (const s of READY) expect(s.blurb, s.id).toBeTruthy();
  });
});

describe("registry views", () => {
  // readySections drops a subsection with no view, so a ready one without it
  // would vanish from the panel.
  it("gives every ready subsection exactly one view", () => {
    for (const section of SECTIONS) {
      for (const sub of section.subsections) {
        if (sub.ready) expect(sub.view, `${section.id}/${sub.id}`).toBeDefined();
      }
    }
  });

  it("keeps Launch › Search a list: its hits are four kinds of row a board cannot hold", () => {
    const launch = READY.find((s) => s.id === "launch");
    expect(launch?.subsections.find((s) => s.id === "search")?.view).toBe(LaunchSearch);
  });
});
