import type { ReactNode } from "react";
import { SearchIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** A quiet line for a tile whose source has nothing, with the glyph that names the source. */
export function Idle({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex h-full flex-col justify-between p-3.5 text-widget-ink-2">
      <span aria-hidden className="[&_svg]:size-6">
        {icon}
      </span>
      <div>
        <p className="m-0 text-notch-w-title leading-tight font-semibold text-widget-ink">{title}</p>
        {children && <p className="m-0 mt-0.5 text-notch-w-meta leading-snug text-pretty">{children}</p>}
      </div>
    </div>
  );
}

/**
 * The widgets look's button: Music's white pill for the one thing a tile is
 * for, a neutral one for the rest. Words always; a glyph only beside them.
 */
export function Pill({
  children,
  onClick,
  quiet = false,
  disabled,
  className,
}: {
  children: ReactNode;
  onClick: () => void;
  quiet?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        NOTCH_FOCUS,
        "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full px-3 text-notch-w-meta font-semibold whitespace-nowrap disabled:opacity-40",
        quiet ? "bg-widget-neutral-track text-widget-ink" : "bg-widget-ink text-widget-on",
        className,
      )}
    >
      {children}
    </button>
  );
}

/** Spotlight's pill, for filtering a board: the same input the calm box is, drawn as a widget. */
export function WidgetSearch({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (value: string) => void;
  /** Both the placeholder and the accessible name. */
  label: string;
}) {
  return (
    <div className="notch-board-bar mb-3 flex h-9 items-center gap-2 rounded-full bg-widget-tile-2 px-3.5 text-widget-ink-2 has-[input:focus-visible]:outline-2 has-[input:focus-visible]:outline-solid has-[input:focus-visible]:outline-notch-focus">
      <SearchIcon className="size-4 shrink-0" strokeWidth={2} aria-hidden />
      <input
        type="text"
        aria-label={label}
        placeholder={label}
        spellCheck={false}
        autoComplete="off"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-full min-w-0 flex-1 bg-transparent font-notch-w text-notch-w-body text-widget-ink outline-none placeholder:text-widget-ink-2"
      />
    </div>
  );
}
