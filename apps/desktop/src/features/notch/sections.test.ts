import { describe, expect, it } from "vitest";
import {
  readySections,
  resolveNav,
  sectionForShortcut,
  panelWidth,
  stepSubsection,
  type Section,
} from "./sections";

const View = () => null;
const section = (id: Section["id"], subs: [string, boolean][]): Section => ({
  id,
  name: id,
  subsections: subs.map(([sub, ready]) => ({ id: sub, name: sub, ready, view: ready ? View : undefined })),
});

const ALL: Section[] = [
  section("now", [["overview", false]]),
  section("agents", [
    ["voice", true],
    ["coding", true],
    ["prs", false],
  ]),
  section("media", [["playing", true]]),
];

describe("readySections", () => {
  // The rule the registry exists for: no dead UI.
  it("hides a section with nothing ready, and the unready subsections of the rest", () => {
    const ready = readySections(ALL);
    expect(ready.map((s) => s.id)).toEqual(["agents", "media"]);
    expect(ready[0].subsections.map((s) => s.id)).toEqual(["voice", "coding"]);
  });

  it("does not show a subsection flagged ready but with no view to draw", () => {
    const flagged: Section[] = [{ ...section("now", []), subsections: [{ id: "x", name: "x", ready: true }] }];
    expect(readySections(flagged)).toEqual([]);
  });
});

describe("resolveNav", () => {
  const ready = readySections(ALL);

  it("lands where it was told, remembering each section's subsection", () => {
    const place = resolveNav(ready, { section: "agents", subs: { agents: "coding" } });
    expect([place?.section.id, place?.sub.id]).toEqual(["agents", "coding"]);
  });

  // A place remembered from a build where the feature existed, or one that
  // is not ready yet, must never open onto nothing.
  it("falls back to the first ready place for a hidden section or subsection", () => {
    expect(resolveNav(ready, { section: "now", subs: {} })?.section.id).toBe("agents");
    expect(resolveNav(ready, { section: "agents", subs: { agents: "prs" } })?.sub.id).toBe("voice");
  });

  it("has nowhere to go when nothing is ready", () => {
    expect(resolveNav([], { section: "now", subs: {} })).toBeNull();
  });
});

describe("sectionForShortcut", () => {
  const ready = readySections(ALL);

  // ⌘2 is Agents in every build, even when Now (⌘1) is hidden in front of it.
  it("numbers by the registry, not by the visible tabs", () => {
    expect(sectionForShortcut(ALL, ready, 2)?.id).toBe("agents");
    expect(sectionForShortcut(ALL, ready, 3)?.id).toBe("media");
  });

  it("does nothing for a hidden section's number or one past the end", () => {
    expect(sectionForShortcut(ALL, ready, 1)).toBeNull();
    expect(sectionForShortcut(ALL, ready, 9)).toBeNull();
  });
});

describe("stepSubsection", () => {
  const agents = readySections(ALL)[0];

  it("steps through ready subsections and wraps", () => {
    expect(stepSubsection(agents, "voice", 1)).toBe("coding");
    expect(stepSubsection(agents, "coding", 1)).toBe("voice");
    expect(stepSubsection(agents, "voice", -1)).toBe("coding");
  });
});

describe("panelWidth", () => {
  it("is 640 up to six sections and 780 beyond, which a widget board also needs", () => {
    expect(panelWidth(6)).toBe(640);
    expect(panelWidth(7)).toBe(780);
  });
});
