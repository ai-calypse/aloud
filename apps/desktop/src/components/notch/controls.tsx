import type { CSSProperties, ReactNode } from "react";
import { Toggle } from "@/components/ui/rows";
import { cn } from "@/lib/utils";

/** The notch's on/off: the app's one switch (ui/rows `Toggle`), in the notch's shape. */
export function Switch(props: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return <Toggle tone="notch" {...props} />;
}

/**
 * A native range input, styled (`.notch-slider` in styles/notch.css): the
 * keyboard, the accessibility tree and the drag all come from the platform.
 * The fill is a gradient stop the browser repaints as the value moves.
 */
export function Slider({
  value,
  onChange,
  label,
  min = 0,
  max = 100,
  step = 1,
  disabled,
  className,
}: {
  value: number;
  onChange: (value: number) => void;
  label: string;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  className?: string;
}) {
  const fill = max > min ? ((value - min) / (max - min)) * 100 : 0;
  return (
    <input
      type="range"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={value}
      disabled={disabled}
      onChange={(e) => onChange(Number(e.target.value))}
      className={cn("notch-slider", className)}
      style={{ "--fill": `${fill}%` } as CSSProperties}
    />
  );
}

/** A key, as the footer and the palette name them: ⌘K, esc, ⇥. */
export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-notch-key bg-notch-well px-[5px]",
        "font-notch-mono text-notch-label leading-none font-medium text-notch-ink-2",
        className,
      )}
    >
      {children}
    </kbd>
  );
}
