import type { Binding, Key, Modifier } from "../../lib/bindings";

/**
 * Renders a `Binding` as word plus symbol — "Cmd ⌘ + D". Side is always
 * spelled out, both sides, because a bare "Option ⌥" would be
 * indistinguishable from the side-agnostic modifier in `MODIFIER_LABEL`.
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

  // A recorded chord pins to specific sides, so those keys win over the
  // side-agnostic modifier. An empty set (presets, hand-written bindings)
  // falls back to the logical symbols, which are either-side by design.
  const exact = binding.exact_modifiers ?? [];
  const modifiers =
    exact.length > 0
      ? exact.map((k) => keyLabel(k))
      : (binding.modifiers ?? []).map((m) => MODIFIER_LABEL[m] ?? m);
  // The key of a modifier-only binding already carries its own label, so
  // prefixing the logical modifier it contributes would print it twice.
  return [...modifiers, keyLabel(binding.key)].join(" + ");
}
