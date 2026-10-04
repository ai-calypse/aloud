import { useState } from "react";
import type { LucideIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { SlidingPill } from "@/components/ui/motion";
import { cn } from "@/lib/utils";
import { onRovingKey } from "./roving";

export type SectionTab = {
  id: string;
  label: string;
  icon: LucideIcon;
  /** How many things here need the user. Absent or 0 draws nothing. */
  count?: number;
  /** The key that reaches it, for the tooltip: "⌘2". */
  shortcut?: string;
};

/**
 * The notch's top-level navigation: icon + label (+ a count where something
 * needs you), one underline that travels on a live spring between them.
 *
 * Labels are never dropped for icons alone — "Named, not memorised"
 * (PRODUCT.md). The strip is one Tab stop; arrows move between sections.
 */
export function SectionTabs({
  items,
  value,
  onChange,
  /** Each tab `aria-controls` this: the one panel the shell swaps. */
  panelId,
  /** ↓ from the strip: hand focus to the body. */
  onExit,
  className,
}: {
  items: SectionTab[];
  value: string;
  onChange: (id: string) => void;
  panelId: string;
  onExit?: () => void;
  className?: string;
}) {
  // A ref callback into state, so the underline re-measures the frame the
  // selected tab exists (see SlidingPill on why `target` is load-bearing).
  const [selected, setSelected] = useState<HTMLButtonElement | null>(null);
  const ids = items.map((item) => item.id);

  return (
    <div
      role="tablist"
      aria-label="Sections"
      className={cn("relative flex min-w-0 gap-0.5", className)}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown" && onExit) {
          e.preventDefault();
          onExit();
          return;
        }
        onRovingKey(e, ids, value, onChange);
      }}
    >
      {items.map(({ id, label, icon: Icon, count, shortcut }) => {
        const on = id === value;
        return (
          <button
            key={id}
            ref={on ? setSelected : undefined}
            type="button"
            role="tab"
            id={`notch-tab-${id}`}
            aria-selected={on}
            aria-controls={panelId}
            tabIndex={on ? 0 : -1}
            data-roving=""
            title={shortcut ? `${label} (${shortcut})` : label}
            onClick={() => onChange(id)}
            className={cn(
              "group relative flex h-10 shrink-0 items-center gap-[5px] rounded-notch-chip px-1.5",
              "font-notch text-notch-body font-medium whitespace-nowrap text-notch-ink-2",
              "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
              "hover:text-notch-ink active:text-notch-ink-2 disabled:pointer-events-none disabled:opacity-40",
              "outline-none",
              NOTCH_FOCUS,
              "focus-visible:outline-offset-[-2px]",
              on && "text-notch-ink",
            )}
          >
            <Icon className="size-3.5 opacity-85" strokeWidth={1.8} aria-hidden />
            {label}
            {count ? (
              <Badge variant="notch-count" aria-label={`${count} need you`}>
                {count}
              </Badge>
            ) : null}
          </button>
        );
      })}
      {/* Rides the strip's bottom hairline (-1px), Vercel's underline. */}
      <SlidingPill
        axis="x"
        tone="bare"
        inset={5}
        target={selected}
        deps={[value, items.length]}
        className="-bottom-px h-0.5 bg-notch-ink"
      />
    </div>
  );
}
