import { describe, expect, it } from "vitest";
import { describeBinding } from "./binding-label";

describe("describeBinding", () => {
  it("shows which side a modifier is on", () => {
    // The whole reason the Swift app bound Right Option specifically: a label
    // of plain "⌥" would make the two indistinguishable in the list.
    expect(
      describeBinding({ kind: "modifierKey", key: "rightOption", modifiers: [] }),
    ).toBe("⌥R");
    expect(
      describeBinding({ kind: "modifierKey", key: "leftOption", modifiers: [] }),
    ).toBe("⌥L");
  });

  it("renders a chord in modifier-then-key order", () => {
    expect(
      describeBinding({
        kind: "key",
        key: { char: "d" },
        modifiers: ["command", "shift"],
      }),
    ).toBe("⌘⇧D");
  });

  it("handles both object-shaped key variants", () => {
    expect(describeBinding({ kind: "key", key: { f: 5 }, modifiers: [] })).toBe(
      "F5",
    );
    expect(
      describeBinding({ kind: "key", key: { char: "j" }, modifiers: [] }),
    ).toBe("J");
  });

  it("names the unbound state rather than rendering empty", () => {
    expect(describeBinding({ kind: "disabled" })).toBe("Not set");
    expect(describeBinding(null)).toBe("Not set");
  });

  it("spells out keys that have no symbol", () => {
    expect(
      describeBinding({ kind: "key", key: "escape", modifiers: [] }),
    ).toBe("esc");
    expect(
      describeBinding({ kind: "key", key: "space", modifiers: ["control"] }),
    ).toBe("⌃space");
  });
});
