import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/** The notch panel's colours (tokens.css, floating surface), by role. */
const NOTCH_COLORS = [
  "notch",
  "notch-ink",
  "notch-ink-2",
  "notch-ink-3",
  "notch-sep",
  "notch-well",
  "notch-edge",
  "notch-track",
  "notch-raised",
  "notch-hover",
  "notch-primary",
  "notch-primary-ink",
  "notch-focus",
  "notch-ok",
  "notch-warn",
  "notch-danger",
];

/**
 * tailwind-merge has to be told about our custom tokens or it silently drops
 * classes: `text-ui` (a size) and `text-brand` (a colour) are both
 * `text-<unknown>` to it, so it files them in one group and keeps only the
 * last. Keep the groups below in step with `@theme inline` in index.css.
 */
const twMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      "font-size": [
        {
          text: [
            "row",
            "ui",
            "hint",
            "notch-lead",
            "notch-body",
            "notch-meta",
            "notch-label",
            "notch-micro",
          ],
        },
      ],
      "font-family": [{ font: ["notch", "notch-mono"] }],
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
            ...NOTCH_COLORS,
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
            ...NOTCH_COLORS,
          ],
        },
      ],
      "border-color": [
        { border: ["surface-border", "separator", "brand", ...NOTCH_COLORS] },
      ],
      // Must stay in step with any `ring-<token>` used in a component: without
      // it tailwind-merge files ring-brand alongside ring-2 and ring-offset-2
      // and drops one of the three.
      "ring-color": [{ ring: ["brand", ...NOTCH_COLORS] }],
      "outline-color": [{ outline: NOTCH_COLORS }],
      rounded: [
        {
          rounded: [
            "card",
            "field",
            "row",
            "chip",
            "notch-key",
            "notch-chip",
            "notch-control",
            "notch-row",
            "notch-track",
            "notch-panel",
          ],
        },
      ],
      "shadow-color": [{ shadow: ["card", "field", "accent", "knob"] }],
    },
  },
});

/** Merge Tailwind class lists, resolving conflicts (shadcn convention). */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
