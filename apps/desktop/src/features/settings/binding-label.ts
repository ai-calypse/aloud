import type { Binding, Key, Modifier } from "../../lib/bindings";

/**
 * Renders a `Binding` as a keycap label.
 *
 * Side matters: Right Option is the popular dictation key precisely because it
 * is not Left Option, so the side is shown rather than collapsed to "⌥". The
 * symbols are the macOS ones, which Windows users also read correctly on a
 * keycap — spelling them out ("Command+Shift+D") makes the cap too wide to sit
 * in the row.
 */

const MODIFIER_SYMBOL: Record<Modifier, string> = {
  command: "⌘",
  control: "⌃",
  option: "⌥",
  shift: "⇧",
  function: "fn",
};

/** Side-specific keys render as symbol + side; the rest speak for themselves. */
const KEY_LABEL: Partial<Record<string, string>> = {
  leftCommand: "⌘L",
  rightCommand: "⌘R",
  leftShift: "⇧L",
  rightShift: "⇧R",
  leftOption: "⌥L",
  rightOption: "⌥R",
  leftControl: "⌃L",
  rightControl: "⌃R",
  fn: "fn",
  escape: "esc",
  space: "space",
};

function keyLabel(key: Key): string {
  if (typeof key === "string") return KEY_LABEL[key] ?? key;
  // Char and F arrive as single-key objects from serde's enum representation.
  if ("char" in key) return String(key.char).toUpperCase();
  if ("f" in key) return `F${key.f}`;
  return "?";
}

/**
 * The side-agnostic modifier a key contributes, mirroring Rust's
 * `Key::logical_modifier`. Both sides collapse to one modifier, which is why
 * holding Left Option cannot mark a Right Option binding: to the matcher they
 * are the same modifier, so there is no extra signal to read.
 */
const KEY_MODIFIER: Partial<Record<string, Modifier>> = {
  leftCommand: "command",
  rightCommand: "command",
  leftShift: "shift",
  rightShift: "shift",
  leftOption: "option",
  rightOption: "option",
  leftControl: "control",
  rightControl: "control",
  fn: "function",
};

/** The modifier `binding` is itself, if it is a modifier-only binding. */
export function bindingModifier(binding: Binding | null): Modifier | null {
  if (!binding || binding.kind !== "modifierKey") return null;
  return typeof binding.key === "string"
    ? (KEY_MODIFIER[binding.key] ?? null)
    : null;
}

export function describeBinding(binding: Binding | null): string {
  if (!binding || binding.kind === "disabled") return "Not set";

  const modifiers = (binding.modifiers ?? []).map((m) => MODIFIER_SYMBOL[m] ?? m);
  // The key of a modifier-only binding already carries its own symbol, so
  // prefixing the logical modifier it contributes would print it twice.
  return [...modifiers, keyLabel(binding.key)].join("");
}
