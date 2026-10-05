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

/** The Widgets look's colours (`widget-*` in index.css), by role. */
const WIDGET_COLORS = [
  "widget-tile",
  "widget-tile-2",
  "widget-tile-deep",
  "widget-neutral-track",
  "widget-sep",
  "widget-radio",
  "widget-ink",
  "widget-ink-2",
  "widget-ink-3",
  "widget-label",
  "widget-on",
  "widget-calendar",
  "widget-media-meta",
  "widget-podcast-meta",
  "widget-acc",
  "widget-acc-ink",
  "widget-fill",
  "widget-ring",
  "widget-track",
  "widget-ev-bg",
  "widget-ev-title",
  "widget-ev-time",
  "widget-ev-bar",
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
            "notch-w-numeral-s",
            "notch-w-numeral-m",
            "notch-w-numeral-l",
            "notch-w-numeral-xl",
            "notch-w-headline",
            "notch-w-body",
            "notch-w-title",
            "notch-w-meta",
            "notch-w-over",
          ],
        },
      ],
      "font-family": [{ font: ["notch", "notch-mono", "notch-w", "notch-w-rounded"] }],
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
            ...WIDGET_COLORS,
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
            ...WIDGET_COLORS,
          ],
        },
      ],
      "border-color": [
        { border: ["surface-border", "separator", "brand", ...NOTCH_COLORS, ...WIDGET_COLORS] },
      ],
      "stroke": [{ stroke: WIDGET_COLORS }],
      "fill": [{ fill: WIDGET_COLORS }],
      // Must stay in step with any `ring-<token>` used in a component: without
      // it tailwind-merge files ring-brand alongside ring-2 and ring-offset-2
      // and drops one of the three.
      "ring-color": [{ ring: ["brand", ...NOTCH_COLORS, ...WIDGET_COLORS] }],
      "outline-color": [{ outline: [...NOTCH_COLORS, ...WIDGET_COLORS] }],
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
            "notch-w-tile",
            "notch-w-card",
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
