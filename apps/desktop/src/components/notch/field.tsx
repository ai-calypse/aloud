import type { InputHTMLAttributes, ReactNode, TextareaHTMLAttributes } from "react";
import { SearchIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const FIELD =
  "w-full min-w-0 rounded-notch-control border border-notch-edge bg-notch-well px-2.5 font-notch " +
  "text-notch-body text-notch-ink outline-none placeholder:text-notch-ink-3 " +
  "disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-notch-focus";

/** A one-line text input in the notch's well: Well ground, Edge border, 26px. */
export function TextField({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      spellCheck={false}
      autoComplete="off"
      {...props}
      className={cn(FIELD, "h-[26px]", className)}
    />
  );
}

/** Its multi-line twin, for text the user keeps (a snippet). */
export function TextArea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      spellCheck={false}
      {...props}
      className={cn(FIELD, "min-h-[84px] resize-none py-1.5 leading-[1.4]", className)}
    />
  );
}

/** A magnifier and an input on the hairline the palette uses, for filtering a list. */
export function SearchBox({
  value,
  onChange,
  label,
  trailing,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  /** Both the placeholder and the accessible name: "Search clipboard". */
  label: string;
  trailing?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex items-center gap-2 border-b border-notch-sep pb-2", className)}>
      <SearchIcon className="size-4 shrink-0 text-notch-ink-3" strokeWidth={1.8} aria-hidden />
      <input
        type="text"
        aria-label={label}
        placeholder={label}
        spellCheck={false}
        autoComplete="off"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          "h-7 min-w-0 flex-1 bg-transparent font-notch text-notch-body text-notch-ink outline-none",
          "placeholder:text-notch-ink-3",
          NOTCH_FOCUS,
          "focus-visible:outline-offset-4",
        )}
      />
      {trailing}
    </div>
  );
}

/**
 * A square of a colour the user picked. The one place a raw colour reaches
 * the notch: it is the user's data — what they sampled — not a palette
 * choice, so it comes from props, not from a token. The Edge border keeps a
 * near-black pick visible on the black panel.
 */
export function Swatch({
  color,
  size = "md",
  className,
}: {
  color: string;
  size?: "md" | "lg";
  className?: string;
}) {
  return (
    <span
      aria-hidden
      style={{ backgroundColor: color }}
      className={cn(
        "inline-block shrink-0 border border-notch-edge",
        size === "lg" ? "size-[30px] rounded-notch-control" : "size-[26px] rounded-notch-control",
        className,
      )}
    />
  );
}
