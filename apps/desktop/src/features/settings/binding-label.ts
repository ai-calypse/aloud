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

export function describeBinding(binding: Binding | null): string {
  if (!binding || binding.kind === "disabled") return "Not set";

  const modifiers = (binding.modifiers ?? []).map((m) => MODIFIER_SYMBOL[m] ?? m);
  // The key of a modifier-only binding already carries its own symbol, so
  // prefixing the logical modifier it contributes would print it twice.
  return [...modifiers, keyLabel(binding.key)].join("");
}
