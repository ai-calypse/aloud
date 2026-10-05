import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  // The failure CLAUDE.md names: tailwind-merge filing a custom size and a
  // custom colour in one `text-*` group and silently keeping only the last.
  it("keeps a notch size and a notch colour side by side", () => {
    expect(cn("text-notch-body", "text-notch-ink-2")).toBe("text-notch-body text-notch-ink-2");
    expect(cn("bg-notch-well", "rounded-notch-chip", "font-notch-mono")).toBe(
      "bg-notch-well rounded-notch-chip font-notch-mono",
    );
  });

  it("still lets a later notch colour override an earlier one", () => {
    expect(cn("text-notch-ink-3", "text-notch-ink")).toBe("text-notch-ink");
    expect(cn("bg-notch-well", "bg-notch-raised")).toBe("bg-notch-raised");
  });

  // Same trap for the Widgets look: a widget size and a widget colour are both
  // `text-<token>`, and the stroke and fill of a ring are not stroke widths.
  it("keeps a widget size, colour and rounded face side by side", () => {
    expect(cn("text-notch-w-numeral-s", "text-widget-acc-ink")).toBe(
      "text-notch-w-numeral-s text-widget-acc-ink",
    );
    expect(cn("font-notch-w-rounded", "font-light")).toBe("font-notch-w-rounded font-light");
    expect(cn("bg-widget-tile", "rounded-notch-w-tile", "stroke-widget-ring", "stroke-2")).toBe(
      "bg-widget-tile rounded-notch-w-tile stroke-widget-ring stroke-2",
    );
  });

  it("still lets a later widget colour override an earlier one", () => {
    expect(cn("bg-widget-tile", "bg-widget-tile-2")).toBe("bg-widget-tile-2");
    expect(cn("text-widget-ink", "text-widget-ink-2")).toBe("text-widget-ink-2");
    expect(cn("stroke-widget-track", "stroke-widget-acc")).toBe("stroke-widget-acc");
  });
});
