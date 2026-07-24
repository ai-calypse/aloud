import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * macOS System Settings building blocks: a caption over a card of
 * hairline-separated rows. The separator is a border-bottom on the row, not a
 * divider element, so a conditional row leaves no stray line behind.
 */
export function Group({
  title,
  children,
  className,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {title && <Caption>{title}</Caption>}
      <Surface className="px-[15px]">{children}</Surface>
    </section>
  );
}

/** The app's card. Solid on purpose: window vibrancy through a grouped list
 * washes out the separators the design is made of. */
export function Surface({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "rounded-card border-[0.5px] border-surface-border bg-surface shadow-card",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** The uppercase label above a group. */
export function Caption({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <h3
      className={cn(
        "mb-[7px] ml-1 text-xs font-semibold uppercase tracking-[0.03em] text-ink-2",
        className,
      )}
    >
      {children}
    </h3>
  );
}

/** A row whose columns are fixed rather than label-left/control-right. */
export function GridRow({
  columns,
  children,
  className,
}: {
  /** A grid-template-columns value, e.g. "150px 1fr auto". */
  columns: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "grid items-center gap-3.5 border-b-[0.5px] border-separator py-3 last:border-b-0",
        className,
      )}
      style={{ gridTemplateColumns: columns }}
    >
      {children}
    </div>
  );
}

/** A label (with optional sub-line) on the left, a control on the right. */
export function Row({
  label,
  hint,
  children,
  className,
}: {
  label?: ReactNode;
  hint?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-b-[0.5px] border-separator py-3 text-row last:border-b-0",
        className,
      )}
    >
      {(label || hint) && (
        <div className="min-w-0">
          {label && <div className="font-medium">{label}</div>}
          {hint && (
            <div className="mt-px text-xs text-ink-2">{hint}</div>
          )}
        </div>
      )}
      {children}
    </div>
  );
}

/** The right-hand control slot: muted text sized to its content, like a popup. */
export function Value({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("min-w-0 max-w-[58%] shrink-0", className)}>
      {children}
    </div>
  );
}

/**
 * Chrome-less shadcn controls, so a Select/Input in a row reads as that row's
 * value rather than a boxed field. `data-[size=default]` is needed because
 * SelectTrigger pins its height by data attribute, which `h-auto` cannot beat.
 */
export const rowSelect =
  "h-auto w-fit max-w-full gap-1 border-0 bg-transparent p-0 text-row text-ink-2 shadow-none data-[size=default]:h-auto focus-visible:ring-0 dark:bg-transparent dark:hover:bg-transparent [&_svg]:text-chevron";

export const rowInput =
  "h-auto rounded-none border-0 bg-transparent p-0 text-right text-row text-ink-2 shadow-none focus-visible:ring-0 md:text-row dark:bg-transparent";

/** Pane heading with an optional subtitle. */
export function PaneTitle({
  title,
  subtitle,
}: {
  title: string;
  subtitle?: string;
}) {
  return (
    <div>
      <h1 className="text-2xl font-bold leading-[1.1] tracking-[-0.02em]">
        {title}
      </h1>
      {subtitle && (
        <p className="mt-1.5 text-ui text-ink-2">{subtitle}</p>
      )}
    </div>
  );
}

/** iOS-style switch: a button with `role="switch"` is the whole component. */
export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  /** Dimmed and unclickable — used when a master switch governs this one. */
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
        "relative h-6 w-10 shrink-0 rounded-full transition-colors duration-[var(--dur-base)] ease-[var(--ease-out)] outline-none",
        "focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2",
        disabled && "opacity-40",
        checked ? "bg-toggle-on" : "bg-toggle-off",
      )}
    >
      <span
        className={cn(
          "absolute left-0.5 top-0.5 size-5 rounded-full bg-white shadow-knob",
          // The overshooting spring, not a decelerating curve: ~1px past the
          // stop and back is what separates an iOS switch from a slider.
          "transition-transform duration-[var(--spring-pop-dur)] ease-[var(--spring-pop)]",
          checked ? "translate-x-4" : "translate-x-0",
        )}
      />
    </button>
  );
}
