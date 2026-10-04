import { useState } from "react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { SlidingPill } from "@/components/ui/motion";
import { cn } from "@/lib/utils";
import { onRovingKey } from "./roving";

export type Segment = { id: string; label: string; disabled?: boolean };

/**
 * A bordered track with a thumb that slides between its segments on a live
 * spring — shadcn's segmented control, Apple's motion. Two roles share it:
 * `SubTabs` navigates (a tablist), `Segmented` picks a value (a radiogroup).
 */
function Track({
  role,
  label,
  items,
  value,
  onChange,
  size,
  panelId,
  className,
}: {
  role: "tablist" | "radiogroup";
  label: string;
  items: Segment[];
  value: string;
  onChange: (id: string) => void;
  size: "md" | "sm";
  panelId?: string;
  className?: string;
}) {
  const [selected, setSelected] = useState<HTMLButtonElement | null>(null);
  const enabled = items.filter((item) => !item.disabled).map((item) => item.id);
  const tabs = role === "tablist";

  return (
    <div
      role={role}
      aria-label={label}
      onKeyDown={(e) => onRovingKey(e, enabled, value, onChange)}
      className={cn(
        "relative inline-flex shrink-0 gap-0.5 border border-notch-edge bg-notch-well",
        size === "md" ? "rounded-notch-track p-[3px]" : "rounded-notch-control p-0.5",
        className,
      )}
    >
      {/* First, so it paints under the labels without a z-index. */}
      <SlidingPill
        axis="x"
        tone="bare"
        target={selected}
        deps={[value, items.length]}
        className={cn(
          "bg-notch-raised",
          size === "md" ? "top-[3px] h-[26px] rounded-notch-chip" : "top-0.5 h-[22px] rounded-notch-chip",
        )}
      />
      {items.map(({ id, label: name, disabled }) => {
        const on = id === value;
        return (
          <button
            key={id}
            ref={on ? setSelected : undefined}
            type="button"
            role={tabs ? "tab" : "radio"}
            id={tabs ? `notch-sub-${id}` : undefined}
            aria-selected={tabs ? on : undefined}
            aria-checked={tabs ? undefined : on}
            aria-controls={tabs ? panelId : undefined}
            tabIndex={on ? 0 : -1}
            disabled={disabled}
            data-roving=""
            onClick={() => onChange(id)}
            className={cn(
              "relative rounded-notch-chip font-notch font-medium whitespace-nowrap text-notch-ink-2",
              "transition-[color,transform] duration-[var(--dur-fast)] ease-[var(--ease-out)]",
              "hover:text-notch-ink active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40",
              "outline-none",
              NOTCH_FOCUS,
              "focus-visible:outline-offset-0",
              size === "md" ? "h-[26px] px-[11px] text-notch-body" : "h-[22px] px-2.5 text-notch-meta",
              on && "text-notch-ink",
            )}
          >
            {name}
          </button>
        );
      })}
    </div>
  );
}

/** The subsections of the open section, under its tab strip. */
export function SubTabs(props: {
  items: Segment[];
  value: string;
  onChange: (id: string) => void;
  panelId: string;
  className?: string;
}) {
  return <Track role="tablist" label="Subsections" size="md" {...props} />;
}

/** A choice of one value inside a view: 30 min · 2 hrs · Always. */
export function Segmented(props: {
  label: string;
  items: Segment[];
  value: string;
  onChange: (id: string) => void;
  className?: string;
}) {
  return <Track role="radiogroup" size="sm" {...props} />;
}
