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

  // Same trap for the widgets: a widget size and a widget colour are both
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

  // Calm Home's tile tokens: a numeral size, its tracking, a state colour and a
  // tile fill all start `text-`/`tracking-`/`bg-` and must survive one another.
  it("keeps a Calm numeral size, tracking, accent and tile together", () => {
    expect(cn("text-notch-n-hero", "text-notch-acc-ink", "tracking-notch-n")).toBe(
      "text-notch-n-hero text-notch-acc-ink tracking-notch-n",
    );
    expect(cn("bg-notch-tile", "rounded-notch-tile", "border-notch-tile-edge")).toBe(
      "bg-notch-tile rounded-notch-tile border-notch-tile-edge",
    );
    expect(cn("text-notch-tile-label", "text-notch-ink-3")).toBe("text-notch-tile-label text-notch-ink-3");
  });

  it("lets a Calm numeral size and tracking replace the widget ones", () => {
    expect(cn("text-notch-w-numeral-l", "text-notch-n-hero")).toBe("text-notch-n-hero");
    expect(cn("tracking-notch-w-numeral", "tracking-notch-n")).toBe("tracking-notch-n");
  });
});
