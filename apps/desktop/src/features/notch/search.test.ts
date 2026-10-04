import { describe, expect, it } from "vitest";
import { BotIcon } from "lucide-react";
import type { NotchAction } from "./actions";
import { paletteEntries, searchPalette } from "./search";
import { readySections, type Section } from "./sections";

const View = () => null;
const ALL: Section[] = [
  {
    id: "agents",
    name: "Agents",
    icon: BotIcon,
    subsections: [
      { id: "coding", name: "Coding", features: ["Claude Code", "Agent approvals"], ready: true, view: View },
      { id: "prs", name: "Pull requests", features: ["GitHub PRs"], ready: false },
    ],
  },
  {
    id: "media",
    name: "Media",
    icon: BotIcon,
    subsections: [{ id: "playing", name: "Now Playing", features: ["Synced lyrics"], ready: false }],
  },
];
const ACTIONS: NotchAction[] = [
  { id: "agents.usage", title: "Show plan usage", keywords: ["quota"], section: "agents", run: () => true },
  { id: "media.play", title: "Play music", section: "media", run: () => true },
  { id: "files.undoTidy", title: "Undo a tidy", args: ["folder"], run: () => true },
];
const entries = paletteEntries(readySections(ALL), ACTIONS);
const labels = (query: string) => searchPalette(entries, query).map((e) => e.label);

describe("paletteEntries", () => {
  // No dead UI: search never offers what the panel does not have.
  it("offers only ready places, and only actions that can run from a keystroke", () => {
    const found = entries.map((e) => e.label);
    expect(found).toContain("Agents");
    expect(found).toContain("Claude Code");
    expect(found).not.toContain("Pull requests");
    expect(found).not.toContain("GitHub PRs");
    expect(found).not.toContain("Media");
    // Hidden section, and an action needing an argument ⌘K cannot give.
    expect(found).not.toContain("Play music");
    expect(found).not.toContain("Undo a tidy");
  });

  it("says where a feature lives", () => {
    expect(entries.find((e) => e.label === "Agent approvals")?.hint).toBe("Agents › Coding");
  });
});

describe("searchPalette", () => {
  it("ranks a label that starts with the query over one that merely contains it", () => {
    // "Coding" starts with "co"; "Claude Code" only has a word that does.
    expect(labels("co").slice(0, 2)).toEqual(["Coding", "Claude Code"]);
  });

  it("finds by keyword and by where it lives, below any label hit", () => {
    expect(labels("quota")).toEqual(["Show plan usage"]);
    expect(labels("agents")[0]).toBe("Agents");
  });

  it("needs every word to land somewhere", () => {
    expect(labels("claude code")).toEqual(["Claude Code"]);
    expect(labels("claude media")).toEqual([]);
  });

  it("is case-blind and finds everything for an empty query", () => {
    expect(labels("CODING")).toContain("Coding");
    expect(searchPalette(entries, "  ").length).toBe(entries.length);
  });
});
