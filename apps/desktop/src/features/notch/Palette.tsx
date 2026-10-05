import { useMemo, useState } from "react";
import { CornerDownLeftIcon, SearchIcon } from "lucide-react";
import { Kbd } from "@/components/notch/controls";
import { cn } from "@/lib/utils";
import { ACTIONS } from "./actions";
import { paletteEntries, searchPalette, type PaletteEntry } from "./search";
import { READY } from "./registry";

/** Built once: the registries never change while the app runs. */
const ENTRIES = paletteEntries(READY, ACTIONS);

/**
 * ⌘K: every section, subsection, feature and action, by name. It opens in
 * the panel's body, in its flow — the panel grows to fit it on its own
 * spring — never as a page-level modal over the user's work.
 */
export function Palette({
  onPick,
  onClose,
}: {
  onPick: (entry: PaletteEntry) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const hits = useMemo(() => searchPalette(ENTRIES, query), [query]);
  const at = Math.min(active, Math.max(0, hits.length - 1));

  return (
    // `data-notch-palette` is what the approval cards check before taking a
    // key (see `fromPalette`).
    <div data-notch-palette="" className="flex flex-col font-notch">
      <div className="flex items-center gap-2 border-b border-notch-sep pb-2.5">
        <SearchIcon className="size-4 shrink-0 text-notch-ink-3" strokeWidth={1.8} aria-hidden />
        <input
          autoFocus
          role="combobox"
          aria-expanded
          aria-controls="notch-palette-list"
          aria-activedescendant={hits[at] ? `notch-palette-${at}` : undefined}
          aria-label="Search sections, features and actions"
          placeholder="Search sections, features and actions"
          spellCheck={false}
          autoComplete="off"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setActive((i) => Math.min(hits.length - 1, Math.max(0, i + step)));
            } else if (e.key === "Enter") {
              e.preventDefault();
              if (hits[at]) onPick(hits[at]);
            } else if (e.key === "Escape") {
              // The palette's Escape, not the panel's: back to the view.
              e.preventDefault();
              e.stopPropagation();
              onClose();
            }
          }}
          className="h-7 min-w-0 flex-1 bg-transparent text-notch-body text-notch-ink outline-none placeholder:text-notch-ink-3"
        />
        <Kbd>esc</Kbd>
      </div>
      <ul
        id="notch-palette-list"
        role="listbox"
        aria-label="Results"
        className="m-0 -mx-2 max-h-[280px] list-none overflow-y-auto p-0 pt-1.5"
      >
        {hits.length === 0 ? (
          <li className="px-2 py-2 text-notch-body text-notch-ink-2">
            Nothing by that name. Try a section: {READY.map((s) => s.name).join(", ")}.
          </li>
        ) : (
          hits.map((entry, i) => (
            <li
              key={entry.key}
              id={`notch-palette-${i}`}
              role="option"
              aria-selected={i === at}
              onMouseMove={() => setActive(i)}
              onClick={() => onPick(entry)}
              className={cn(
                "flex min-h-8 cursor-pointer items-center gap-3 rounded-notch-row px-2 text-notch-body",
                i === at && "bg-notch-well",
              )}
            >
              <span className="min-w-0 flex-1 truncate text-notch-ink">{entry.label}</span>
              <span className="shrink-0 truncate text-notch-meta text-notch-ink-3">{entry.hint}</span>
              {i === at && (
                <CornerDownLeftIcon className="size-3.5 shrink-0 text-notch-ink-3" aria-hidden />
              )}
            </li>
          ))
        )}
      </ul>
    </div>
  );
}
