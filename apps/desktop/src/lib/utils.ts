import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/**
 * tailwind-merge has to be told about our custom tokens or it silently drops
 * classes: `text-ui` (a size) and `text-brand` (a colour) are both
 * `text-<unknown>` to it, so it files them in one group and keeps only the
 * last. Keep the groups below in step with `@theme inline` in index.css.
 */
const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      "font-size": [{ text: ["row", "ui", "hint"] }],
      "text-color": [
        {
          text: [
            "ink",
            "ink-2",
            "ink-3",
            "brand",
            "brand-fg",
            "danger",
            "success",
            "warning",
            "chevron",
            "search-icon",
            "app-action",
            "app-action-fg",
            "bubble",
            "bubble-fg",
            "folder",
          ],
        },
      ],
      "bg-color": [
        {
          bg: [
            "canvas",
            "surface",
            "field",
            "hover",
            "brand",
            "brand-fg",
            "danger",
            "success",
            "warning",
            "warning-bg",
            "toggle-on",
            "toggle-off",
            "separator",
            "app-action",
            "app-action-fg",
            "bubble",
            "bubble-fg",
            "folder",
          ],
        },
      ],
      "border-color": [{ border: ["surface-border", "separator", "brand"] }],
      // Must stay in step with any `ring-<token>` used in a component: without
      // it tailwind-merge files ring-brand alongside ring-2 and ring-offset-2
      // and drops one of the three.
      "ring-color": [{ ring: ["brand"] }],
      rounded: [{ rounded: ["card", "field", "row", "chip"] }],
      "shadow-color": [{ shadow: ["card", "field", "accent", "knob"] }],
    },
  },
});

/** Merge Tailwind class lists, resolving conflicts (shadcn convention). */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
