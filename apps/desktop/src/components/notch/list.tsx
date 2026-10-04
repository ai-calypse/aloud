import type { ReactNode } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/**
 * Rows on hairlines: the notch's one list. No card around it and no card per
 * row — a panel hanging off the notch is already the container, and a card
 * in it would be a card in a card.
 */
export function List({
  children,
  label,
  className,
}: {
  children: ReactNode;
  /** Read by assistive tech for the list as a whole; `Label` is the visible one. */
  label?: string;
  className?: string;
}) {
  return (
    <ul aria-label={label} className={cn("m-0 list-none p-0", className)}>
      {children}
    </ul>
  );
}

/**
 * One line of a `List`: a lead (icon, art, a check), a title over an optional
 * subtitle, and a trailing value or action.
 *
 * With `onClick` the whole row is one button — then `trailing` must not hold
 * another control (a button in a button is invalid and unreachable). A row
 * with its own actions takes no `onClick`.
 */
export function Row({
  lead,
  title,
  subtitle,
  trailing,
  onClick,
  selected,
  className,
}: {
  lead?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  trailing?: ReactNode;
  onClick?: () => void;
  selected?: boolean;
  className?: string;
}) {
  const body = (
    <>
      {lead}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-notch-body text-notch-ink">{title}</span>
        {subtitle && (
          <span className="block truncate text-notch-meta text-notch-ink-2">{subtitle}</span>
        )}
      </span>
      {trailing}
    </>
  );
  const shape = cn(
    "flex min-h-[38px] w-full items-center gap-2.5 rounded-notch-row px-2 py-1.5 text-left font-notch",
    "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
    selected && "bg-notch-well",
  );
  return (
    // The hairline is the item's own border, not a divider element, so a
    // conditional row leaves no stray line behind (as in ui/rows).
    <li className={cn("border-b border-notch-sep last:border-b-0", className)}>
      {onClick ? (
        <button
          type="button"
          onClick={onClick}
          aria-current={selected || undefined}
          className={cn(
            shape,
            "outline-none hover:bg-notch-hover active:bg-notch-well disabled:opacity-40",
            NOTCH_FOCUS,
            "focus-visible:outline-offset-[-2px]",
          )}
        >
          {body}
        </button>
      ) : (
        <div className={shape}>{body}</div>
      )}
    </li>
  );
}

/**
 * A row's trailing number: tabular mono, so a column of them lines up and a
 * ticking value does not jitter.
 */
export function Value({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "shrink-0 font-notch-mono text-notch-meta whitespace-nowrap text-notch-ink-2 tabular-nums",
        className,
      )}
    >
      {children}
    </span>
  );
}

/**
 * The one fact a view leads with — a time, a percentage, a count — and the
 * line under it. One per view: two leads is no lead.
 */
export function Lead({
  children,
  sub,
  className,
}: {
  children: ReactNode;
  sub?: ReactNode;
  className?: string;
}) {
  return (
    <div className={className}>
      <div className="font-notch text-notch-lead leading-[1.1] font-semibold tracking-[-0.035em] text-notch-ink tabular-nums">
        {children}
      </div>
      {sub && <div className="mt-1 text-notch-body text-notch-ink-2">{sub}</div>}
    </div>
  );
}

/**
 * A view with nothing to show. It teaches: what puts something here, and the
 * one thing to do about it — never a bare "nothing here".
 */
export function Empty({
  title,
  children,
  action,
  className,
}: {
  title: ReactNode;
  /** What puts something here, in a sentence. */
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-start gap-1 py-1 font-notch", className)}>
      <p className="m-0 text-notch-body font-medium text-notch-ink">{title}</p>
      <p className="m-0 max-w-[56ch] text-notch-body text-pretty text-notch-ink-2">{children}</p>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/**
 * What a group of rows is, above it. Sentence case, not a tracked uppercase
 * eyebrow: it names, it does not shout.
 */
export function Label({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <p className={cn("m-0 mb-1.5 font-notch text-notch-label font-medium text-notch-ink-3", className)}>
      {children}
    </p>
  );
}
