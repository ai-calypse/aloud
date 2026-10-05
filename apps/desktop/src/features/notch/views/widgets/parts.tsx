import type { ButtonHTMLAttributes, ReactNode } from "react";
import { BluetoothIcon, HeadphonesIcon, KeyboardIcon, MouseIcon, TouchpadIcon, type LucideIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * What every Media widget leans on: the notice a tile shows when its source has
 * nothing or needs something from the user, and the pill that acts on it.
 */

/**
 * A tile that says plainly what it lacks and what would fill it, and carries
 * the one thing to do about it. The body scrolls rather than clipping: the
 * browser-permission fixes are long.
 */
export function Notice({
  icon,
  eyebrow,
  title,
  children,
  actions,
}: {
  icon: ReactNode;
  eyebrow?: string;
  title: string;
  children?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="flex h-full flex-col gap-2 p-3.5">
      <span aria-hidden className="text-widget-ink-2 [&_svg]:size-6">
        {icon}
      </span>
      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {eyebrow && (
          <span className="truncate text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
            {eyebrow}
          </span>
        )}
        <span className="text-notch-w-headline leading-tight font-semibold text-pretty">{title}</span>
        {children && (
          <div className="flex flex-col gap-1.5 text-notch-w-meta leading-snug text-pretty text-widget-ink-2">
            {children}
          </div>
        )}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

/**
 * A capsule button. `solid` is the one thing a tile is for (white with dark
 * words, because Apple's translucent pill is under 3:1); the rest are a veil
 * of the ink.
 */
export function Pill({
  solid = false,
  className,
  ...props
}: { solid?: boolean } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        NOTCH_FOCUS,
        "inline-flex h-7.5 shrink-0 items-center gap-1.5 rounded-full px-3.5 text-notch-w-title font-semibold",
        "transition-opacity duration-[var(--dur-fast)] ease-[var(--ease-out)] disabled:opacity-50",
        solid ? "bg-widget-ink text-widget-on" : "bg-widget-ink/15 text-widget-ink",
        className,
      )}
    />
  );
}

/** The glyph a Bluetooth device's kind earns, as the Batteries widget draws one. */
const KIND: Record<string, LucideIcon> = {
  headphones: HeadphonesIcon,
  mouse: MouseIcon,
  keyboard: KeyboardIcon,
  trackpad: TouchpadIcon,
};

export function kindIcon(kind: string): LucideIcon {
  return KIND[kind] ?? BluetoothIcon;
}
