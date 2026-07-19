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
      {title && (
        <h3 className="mb-[7px] ml-1 text-xs font-semibold uppercase tracking-[0.03em] text-ink-2">
          {title}
        </h3>
      )}
      <div className="rounded-card border-[0.5px] border-surface-border bg-surface px-[15px] shadow-card">
        {children}
      </div>
    </section>
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
        "relative h-6 w-10 shrink-0 rounded-full transition-colors duration-300 outline-none",
        "focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2",
        checked ? "bg-toggle-on" : "bg-toggle-off",
      )}
    >
      <span
        className={cn(
          "absolute left-0.5 top-0.5 size-5 rounded-full bg-white shadow-knob",
          "transition-transform duration-300 ease-[cubic-bezier(.32,.72,0,1)]",
          checked ? "translate-x-4" : "translate-x-0",
        )}
      />
    </button>
  );
}
