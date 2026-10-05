import type { CSSProperties, ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** Where a tile sits on Home's 8×3 grid, and which of its layouts it draws. */
export type TilePlacement = {
  /** Names the tile for `useFlip`, which finds tiles by `data-tile`. */
  id: string;
  col: number;
  row: number;
  w: number;
  h: number;
  size: "L" | "M" | "S";
};

/**
 * Calm's tile frame: flat black on the panel's black, a hairline, 12px
 * corners, the 12px padding every tile body is laid out for. It replaces
 * `WidgetFrame`, whose geometry is Apple's fixed 158pt squares; this one
 * fills whatever cell the plan gives it.
 *
 * `state` is the only way an accent enters (`data-tile-state`, tokens.css),
 * and the body must say the same thing in a word: colour is never the only
 * carrier. Everything a tile draws in its accent (ring, capsule, label glyph)
 * follows it through the widget primitives' variables.
 *
 * With `onActivate` the tile opens what it summarises. The one tab stop is a
 * button stretched over the whole tile and *behind* its content, so the
 * tile's own controls (a play button, a scrubber) are siblings of it and not
 * inside it; the content lets the pointer through except on those controls.
 *
 * A change of `size` re-keys the content so it crossfades (notch.css); the
 * tile itself is never scaled.
 */
export function Tile({
  placement,
  state,
  label,
  onActivate,
  className,
  children,
}: {
  placement: TilePlacement;
  state?: "ok" | "caution" | "alert";
  /** The tile's accessible name; the visible label is the body's `TileLabel`. */
  label: string;
  onActivate?: () => void;
  className?: string;
  children: ReactNode;
}) {
  const style: CSSProperties = {
    gridColumn: `${placement.col} / span ${placement.w}`,
    gridRow: `${placement.row} / span ${placement.h}`,
  };
  return (
    <section
      aria-label={label}
      data-notch-tile=""
      data-tile={placement.id}
      data-size={placement.size}
      data-tile-state={state}
      data-actionable={onActivate ? "" : undefined}
      style={style}
      className={cn(
        "notch-tile rounded-notch-tile border border-notch-tile-edge bg-notch-tile p-3 text-notch-ink",
        className,
      )}
    >
      {onActivate && (
        <button
          type="button"
          aria-label={label}
          onClick={onActivate}
          className={cn(
            NOTCH_FOCUS,
            "absolute inset-0 cursor-pointer rounded-notch-tile outline-none focus-visible:outline-offset-[-2px]",
          )}
        />
      )}
      <div
        key={placement.size}
        className="notch-tile__in pointer-events-none relative flex h-full min-h-0 flex-col [&_button]:pointer-events-auto [&_input]:pointer-events-auto"
      >
        {children}
      </div>
    </section>
  );
}

/** A tile's name, its glyph in the tile's accent: 11px semibold, the prototype's `.lbl`. */
export function TileLabel({
  icon,
  children,
  className,
}: {
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-1 text-notch-tile-label leading-tight font-semibold whitespace-nowrap text-notch-ink-3",
        className,
      )}
    >
      {icon && (
        <span aria-hidden className="text-notch-acc [&_svg]:size-3.25">
          {icon}
        </span>
      )}
      <span className="min-w-0 truncate">{children}</span>
    </div>
  );
}
