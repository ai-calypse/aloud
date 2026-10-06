import { describe, expect, it } from "vitest";
import { shellIntent, type ShellKey } from "./keys";

const key = (k: Partial<ShellKey> & { key: string }): ShellKey => ({
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  editing: false,
  inBody: false,
  ...k,
});

describe("shellIntent", () => {
  it("switches section on ⌘1–9, even from a text field", () => {
    expect(shellIntent(key({ key: "2", metaKey: true }))).toEqual({ kind: "section", digit: 2 });
    expect(shellIntent(key({ key: "9", metaKey: true, editing: true }))).toEqual({
      kind: "section",
      digit: 9,
    });
    // ⌘0 and a bare digit are not ours.
    expect(shellIntent(key({ key: "0", metaKey: true }))).toBeNull();
    expect(shellIntent(key({ key: "2" }))).toBeNull();
  });

  it("opens the palette on ⌘K", () => {
    expect(shellIntent(key({ key: "k", metaKey: true }))).toEqual({ kind: "palette" });
    expect(shellIntent(key({ key: "K", metaKey: true, shiftKey: true }))).toEqual({ kind: "palette" });
  });

  it("steps subsections on Tab and Shift-Tab from the chrome", () => {
    expect(shellIntent(key({ key: "Tab" }))).toEqual({ kind: "sub", delta: 1 });
    expect(shellIntent(key({ key: "Tab", shiftKey: true }))).toEqual({ kind: "sub", delta: -1 });
  });

  // The failure this guards: an approval card whose Allow button cannot be
  // reached by keyboard, because Tab was taken for navigation.
  it("leaves Tab to the body and to text fields", () => {
    expect(shellIntent(key({ key: "Tab", inBody: true }))).toBeNull();
    expect(shellIntent(key({ key: "Tab", editing: true }))).toBeNull();
  });

  it("does not claim Option chords", () => {
    expect(shellIntent(key({ key: "1", metaKey: true, altKey: true }))).toBeNull();
    expect(shellIntent(key({ key: "Tab", altKey: true }))).toBeNull();
  });
});
