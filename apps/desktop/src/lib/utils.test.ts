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
});
