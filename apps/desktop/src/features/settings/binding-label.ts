import type { Binding, Key, Modifier } from "../../lib/bindings";

/**
 * Renders a `Binding` as a label carrying both the word and the symbol —
 * "Cmd ⌘ + D" — mirroring the Swift app's `orderedDisplayNames`. The symbol
 * alone ("⌘D") is compact but the word is what most people actually read.
 *
 * Side is always spelled out — "Left Option ⌥", "Right Option ⌥" — because a
 * recorded chord pins to one side, and a bare "Option ⌥" would be
 * indistinguishable from the side-agnostic modifier (which is exactly what the
 * generic `MODIFIER_LABEL` below is for). Both sides get a prefix so neither
 * reads as the default.
 */

const MODIFIER_LABEL: Record<Modifier, string> = {
  command: "Cmd ⌘",
  control: "Ctrl ⌃",
  option: "Option ⌥",
  shift: "Shift ⇧",
  function: "Fn 🌐",
};

/** Side-specific keys spell out the side; the rest speak for themselves. */
const KEY_LABEL: Partial<Record<string, string>> = {
  leftCommand: "Left Cmd ⌘",
  rightCommand: "Right Cmd ⌘",
  leftShift: "Left Shift ⇧",
  rightShift: "Right Shift ⇧",
  leftOption: "Left Option ⌥",
  rightOption: "Right Option ⌥",
  leftControl: "Left Ctrl ⌃",
  rightControl: "Right Ctrl ⌃",
  fn: "Fn 🌐",
  escape: "Esc",
  space: "Space",
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

  // A recorded chord pins to specific sides, so render those keys (Right Option
  // reads distinctly from the left) rather than the side-agnostic modifier. An
  // empty set — presets, hand-written bindings — falls back to the logical
  // symbols, which is either-side by design.
  const exact = binding.exact_modifiers ?? [];
  const modifiers =
    exact.length > 0
      ? exact.map((k) => keyLabel(k))
      : (binding.modifiers ?? []).map((m) => MODIFIER_LABEL[m] ?? m);
  // The key of a modifier-only binding already carries its own label, so
  // prefixing the logical modifier it contributes would print it twice.
  return [...modifiers, keyLabel(binding.key)].join(" + ");
}
