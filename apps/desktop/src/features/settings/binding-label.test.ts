import { describe, expect, it } from "vitest";
import { describeBinding } from "./binding-label";

describe("describeBinding", () => {
  it("shows which side a modifier is on", () => {
    // The whole reason the Swift app bound Right Option specifically: a label
    // of plain "⌥" would make the two indistinguishable in the list.
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
    // `exact_modifiers` is what a recorded chord pins to, and it has to win:
    // rendering `modifiers` as well would print the Command twice, once
    // side-agnostic and once not. Untested until now, which is how the format
    // change below went unnoticed.
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
