import { describe, expect, it } from "vitest";
import { describeBinding } from "./binding-label";

describe("describeBinding", () => {
  it("shows which side a modifier is on", () => {
    // A label of plain "⌥" makes the two sides indistinguishable in the list.
    expect(
      describeBinding({ kind: "modifierKey", key: "rightOption", modifiers: [] }),
    ).toBe("Right Option ⌥");
    expect(
      describeBinding({ kind: "modifierKey", key: "leftOption", modifiers: [] }),
    ).toBe("Left Option ⌥");
  });

  it("renders a chord in modifier-then-key order", () => {
    expect(
      describeBinding({
        kind: "key",
        key: { char: "d" },
        modifiers: ["command", "shift"],
      }),
    ).toBe("Cmd ⌘ + Shift ⇧ + D");
  });

  it("prefers the recorded side over the logical modifier", () => {
    // `exact_modifiers` has to win: rendering `modifiers` too prints the
    // Command twice, once side-agnostic and once not.
    expect(
      describeBinding({
        kind: "key",
        key: { char: "d" },
        modifiers: ["command"],
        exact_modifiers: ["rightCommand"],
      }),
    ).toBe("Right Cmd ⌘ + D");
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
    ).toBe("Esc");
    expect(
      describeBinding({ kind: "key", key: "space", modifiers: ["control"] }),
    ).toBe("Ctrl ⌃ + Space");
  });
});
