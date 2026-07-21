import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * macOS System Settings building blocks: an uppercase caption over a solid
 * rounded card whose children are hairline-separated rows.
 *
 * The separator is drawn by the row (border-bottom, dropped on the last child)
 * rather than by a divider element, so a conditionally-rendered row can appear
 * and disappear without leaving a stray line behind.
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

/**
 * The app's card: a solid panel with a hairline border.
 *
 * Solid on purpose — letting the window vibrancy through a grouped list washes
 * out the separators the design is made of.
 */
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

/**
 * A row whose columns are fixed rather than label-left/control-right.
 *
 * Five places grew their own copy of this grid — hotkeys, macros, app rules,
 * app-rule suggestions, connectors — each repeating the same hairline and
 * last-child rule with a different column template.
 */
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
 * Chrome-less variants of the shadcn controls, so a Select/Input sitting in a
 * row reads as the right-hand value of that row rather than as a boxed field.
 * The `data-[size=default]` override is needed because SelectTrigger pins its
 * own height by data attribute, which a plain `h-auto` cannot outrank.
 */
export const rowSelect =
  "h-auto w-fit max-w-full gap-1 border-0 bg-transparent p-0 text-row text-ink-2 shadow-none data-[size=default]:h-auto focus-visible:ring-0 dark:bg-transparent dark:hover:bg-transparent [&_svg]:text-chevron";

export const rowInput =
  "h-auto rounded-none border-0 bg-transparent p-0 text-right text-row text-ink-2 shadow-none focus-visible:ring-0 md:text-row dark:bg-transparent";

/** Pane heading, matching the mock's 24px/700 title and optional subtitle. */
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

/**
 * iOS-style switch. Radix has no Switch in this project's `components/ui`, and
 * a checkbox reads wrong for the immediate on/off rows in the design — a
 * button with `role="switch"` is the whole component.
 */
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-6 w-10 shrink-0 rounded-full transition-colors duration-[var(--dur-base)] ease-[var(--ease-out)] outline-none",
        "focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2",
        checked ? "bg-toggle-on" : "bg-toggle-off",
      )}
    >
      <span
        className={cn(
          "absolute left-0.5 top-0.5 size-5 rounded-full bg-white shadow-knob",
          // The knob is a physical object being thrown to the other end of the
          // track, so it gets the spring that overshoots rather than a curve
          // that decelerates: ~1px past the stop and back, which is the whole
          // difference between an iOS switch and a checkbox that slides.
          "transition-transform duration-[var(--spring-pop-dur)] ease-[var(--spring-pop)]",
          checked ? "translate-x-4" : "translate-x-0",
        )}
      />
    </button>
  );
}
