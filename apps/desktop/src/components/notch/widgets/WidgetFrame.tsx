import type { MouseEvent, ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { WidgetRole } from "./roles";

export type WidgetSize = "small" | "medium" | "large";

/**
 * A board: four columns of small tiles, medium and large spanning two. Dense,
 * so a small tile fills the hole a medium leaves. The grid's rows are a tile
 * and its label.
 */
export function WidgetBoard({
  label,
  children,
  className,
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div role="group" aria-label={label} className={cn("notch-board", className)}>
      {children}
    </div>
  );
}

/** Where the tile's neutral ground comes from when it has no live treatment. */
const GROUND = {
  tile: "bg-widget-tile",
  vibrant: "bg-widget-tile-2",
  deep: "bg-widget-tile-deep",
} as const;

/**
 * The shell of every widget: a fixed-size rounded tile with its label under
 * it, as Apple draws them (small 158pt square, medium 338x158, large 338x354).
 * Sizes are fixed on purpose, so a board is a grid rather than a ranking.
 *
 * `live` is the gauge treatment from the Figma dashboards: one theme hue per
 * tile, the tile its deepest shade, and everything drawn in it (ring,
 * numerals, track) in the lightest tint. It needs an `accent`, and says
 * something is happening; it does not replace the number that says what.
 *
 * With `onActivate` the tile opens something. The label under it becomes the
 * button, so the keyboard has one stop and the tile's own controls (a play
 * pill) are never nested in another button; a click on the tile itself, away
 * from those controls, does the same for the pointer.
 */
export function WidgetFrame({
  size,
  label,
  accent,
  live = false,
  ground = "tile",
  onActivate,
  children,
  className,
}: {
  size: WidgetSize;
  /** Under the tile, and its accessible name. */
  label: string;
  accent?: WidgetRole;
  live?: boolean;
  ground?: keyof typeof GROUND;
  onActivate?: () => void;
  children: ReactNode;
  className?: string;
}) {
  const open = (e: MouseEvent<HTMLElement>) => {
    if (e.target instanceof Element && e.target.closest("button, a, input, select, textarea")) return;
    onActivate?.();
  };
  return (
    <div
      // Medium and large are two columns wide, large two rows tall, in a
      // board's four-column grid.
      className={cn(
        "flex min-w-0 flex-col items-center",
        size !== "small" && "col-span-2",
        size === "large" && "row-span-2",
        className,
      )}
    >
      <section
        aria-label={label}
        data-size={size}
        data-w-role={accent}
        data-w-live={live && accent ? "" : undefined}
        data-actionable={onActivate ? "" : undefined}
        onClick={onActivate ? open : undefined}
        className={cn(
          "notch-widget rounded-notch-w-tile font-notch-w text-widget-ink",
          live && accent ? "bg-widget-tile" : GROUND[ground],
          onActivate && "cursor-pointer",
        )}
      >
        {children}
      </section>
      {onActivate ? (
        <button
          type="button"
          onClick={onActivate}
          className={cn(
            NOTCH_FOCUS,
            "mt-1.5 rounded-notch-chip px-1.5 text-notch-w-meta leading-none font-medium whitespace-nowrap text-widget-label",
          )}
        >
          {label}
        </button>
      ) : (
        <span className="mt-1.5 text-notch-w-meta leading-none font-medium whitespace-nowrap text-widget-label">
          {label}
        </span>
      )}
    </div>
  );
}
