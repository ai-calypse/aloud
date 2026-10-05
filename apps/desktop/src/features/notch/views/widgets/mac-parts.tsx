import type { ButtonHTMLAttributes, ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { onRovingKey } from "@/components/notch/roving";
import { cn } from "@/lib/utils";

/**
 * Small controls shared by the Mac and Focus widgets. They are the widgets
 * look's own (white pill, grey chip), not the calm `Button` and `Segmented`,
 * which are drawn in the calm palette and would look pasted onto a tile.
 */

/** A quiet line for a tile whose source has nothing, with the glyph that names the source. */
export function Idle({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="flex h-full flex-col justify-between p-3.5 text-widget-ink-2">
      <span aria-hidden className="[&_svg]:size-6">
        {icon}
      </span>
      <span className="text-notch-w-title leading-tight font-medium">{children}</span>
    </div>
  );
}

/**
 * A pill. The primary is white with dark words: Apple's translucent one is
 * under 3:1 (the same call the Music tile makes). Quiet is the neutral track.
 */
export function Pill({
  primary = false,
  className,
  ...props
}: { primary?: boolean } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        NOTCH_FOCUS,
        "inline-flex h-7.5 shrink-0 items-center justify-center gap-1.5 rounded-full px-3.5 text-notch-w-title font-semibold whitespace-nowrap",
        "transition-transform duration-[var(--dur-fast)] ease-[var(--ease-out)] active:scale-97",
        "disabled:pointer-events-none disabled:opacity-40",
        primary ? "bg-widget-ink text-widget-on" : "bg-widget-neutral-track text-widget-ink",
        className,
      )}
    />
  );
}

/**
 * A choice of one value in a tile: a radiogroup of chips, one Tab stop with
 * arrows to move, as the calm `Segmented` is. The chosen chip is the white
 * pill, so the choice never rests on colour alone.
 */
export function Choice({
  label,
  items,
  value,
  onChange,
  className,
}: {
  label: string;
  items: { id: string; label: string }[];
  value: string;
  onChange: (id: string) => void;
  className?: string;
}) {
  const ids = items.map((i) => i.id);
  return (
    <div
      role="radiogroup"
      aria-label={label}
      onKeyDown={(e) => onRovingKey(e, ids, value, onChange)}
      className={cn("flex flex-wrap gap-1", className)}
    >
      {items.map((item) => {
        const on = item.id === value;
        return (
          <button
            key={item.id}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on || !ids.includes(value) ? 0 : -1}
            data-roving=""
            onClick={() => onChange(item.id)}
            className={cn(
              NOTCH_FOCUS,
              "h-6 rounded-full px-2.5 text-notch-w-meta font-semibold whitespace-nowrap",
              "transition-[color,background-color,transform] duration-[var(--dur-fast)] ease-[var(--ease-out)] active:scale-97",
              on ? "bg-widget-ink text-widget-on" : "bg-widget-neutral-track text-widget-ink",
            )}
          >
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The widget boards' on/off. The ordinary `Switch` is a raised grey on the notch's
 * black, which vanishes on a #252525 tile when off; this one is a white track
 * with a dark knob when on and the neutral track with a white knob when off,
 * so the state shows in the knob's side as well as its tone.
 */
export function WidgetSwitch({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        NOTCH_FOCUS,
        "relative h-5 w-8.5 shrink-0 rounded-full transition-colors duration-[var(--dur-base)] ease-[var(--ease-out)]",
        "disabled:opacity-40",
        checked ? "bg-widget-ink" : "bg-widget-neutral-track",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 size-4 rounded-full",
          // The overshooting spring the calm switch uses: about a pixel past the stop and back.
          "transition-transform duration-[var(--spring-pop-dur)] ease-[var(--spring-pop)]",
          checked ? "translate-x-3.5 bg-widget-on" : "translate-x-0 bg-widget-ink",
        )}
      />
    </button>
  );
}
