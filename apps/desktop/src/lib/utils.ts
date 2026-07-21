import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/**
 * tailwind-merge has to be told about our custom tokens, or it silently drops
 * classes.
 *
 * It resolves conflicts by parsing a class into a group — `text-sm` and
 * `text-lg` are both font-size, so the later wins. It knows Tailwind's own
 * scale, but `text-ui` (our 13px size) and `text-brand` (our accent colour)
 * are both just `text-<unknown>` to it, so it files them in the same group and
 * keeps only the last. That is how `<Button variant="plain" size="text">` lost
 * its blue: `text-ui` from the size arrived after `text-brand` from the variant
 * and evicted it.
 *
 * Declaring the groups below is the fix, and it has to be kept in step with the
 * `@theme inline` block in index.css.
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
