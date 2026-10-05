import type React from "react";

/**
 * Where an arrow key moves a roving focus in a horizontal strip of `count`
 * items, or null when the key is not one of ours. Wraps at both ends, as a
 * macOS segmented control does. Pure, so the strip's keyboard is testable
 * without a DOM.
 */
export function rovingIndex(key: string, index: number, count: number): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "ArrowRight":
      return (index + 1) % count;
    case "ArrowLeft":
      return (index - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/**
 * The keydown for a strip whose items are `[role=tab]` or `[role=radio]`
 * buttons: arrows move focus *and* select (automatic activation — every view
 * here is cheap to show), so the strip is one Tab stop, not one per item.
 */
export function onRovingKey(
  event: React.KeyboardEvent<HTMLElement>,
  ids: string[],
  value: string,
  onChange: (id: string) => void,
) {
  const next = rovingIndex(event.key, ids.indexOf(value), ids.length);
  if (next === null) return;
  event.preventDefault();
  onChange(ids[next]);
  const items = event.currentTarget.querySelectorAll<HTMLElement>("[data-roving]");
  items[next]?.focus();
}
