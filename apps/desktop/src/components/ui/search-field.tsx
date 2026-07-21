import { SearchIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * The app's search input: a magnifier in a bordered well.
 *
 * Existed twice, byte-identical down to the SVG path data — once in the Notes
 * list, once in the Connectors pane — which is how the two drifted to different
 * vertical padding.
 */
export function SearchField({
  value,
  onChange,
  placeholder,
  className,
  autoFocus,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  className?: string;
  autoFocus?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex items-center gap-[7px] rounded-field border-[0.5px] border-surface-border bg-surface px-3 py-2 shadow-field",
        "focus-within:border-brand",
        className,
      )}
    >
      <SearchIcon
        className="size-[13px] shrink-0 text-search-icon"
        strokeWidth={1.6}
        aria-hidden="true"
      />
      <input
        type="search"
        className="w-full bg-transparent text-ui outline-none placeholder:text-ink-2"
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        autoFocus={autoFocus}
      />
    </div>
  );
}
