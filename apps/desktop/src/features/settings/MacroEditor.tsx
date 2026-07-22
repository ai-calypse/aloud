import { type ReactNode } from "react";
import { type Macro } from "../../lib/bindings";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { GridRow, rowInput } from "@/components/ui/rows";
import { DeleteRow, EditableList } from "@/components/ui/editable-list";

/**
 * Replacements: a spoken phrase and the text it always becomes — the
 * *deterministic* mechanism, run on the finished transcript. The probabilistic
 * ones are the two `TermListEditor` lists above it.
 *
 * A replacement that cannot fire looks identical to one that can, which is how
 * a row with a blank phrase sat in a real settings file unnoticed. So every row
 * that is off says why.
 */

/**
 * Mirrors `fold` in `crates/store/src/macros.rs`, which is what actually decides
 * whether two phrases collide: apostrophes dropped, whitespace runs merged,
 * case folded.
 *
 * ponytail: a second copy of that rule, kept because the alternative is an IPC
 * round-trip per keystroke to label a warning. A `commands.foldTrigger` is the
 * upgrade path if the two ever drift.
 */
const fold = (s: string) =>
  s
    .replace(/['‘’ʼ]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();

export function MacroEditor({
  macros,
  onChange,
  children,
}: {
  macros: Macro[];
  onChange: (macros: Macro[]) => void;
  children?: ReactNode;
}) {
  const folded = macros.map((m) => fold(m.trigger));

  return (
    <EditableList
      title="Shortcuts"
      items={macros}
      onChange={onChange}
      make={() => ({ trigger: "", expansion: "" })}
      isBlank={(m) => !m.trigger.trim() && !m.expansion.trim()}
      problem={(m, i) => {
        if (!m.trigger.trim())
          return "Add a phrase to say. Until then this does nothing.";
        if (!m.expansion.trim())
          return "Add the text it should become. Until then this does nothing.";
        return folded.indexOf(folded[i]) !== i
          ? "Another row already claims this phrase. The first one wins."
          : null;
      }}
      addLabel="Add Shortcut"
      empty={{
        title: "No Shortcuts Yet",
        body: "Say a short phrase while dictating; Aloud types the long one.",
      }}
      row={({ item, autoFocus, set, remove }) => (
        <GridRow
          columns="minmax(0,180px) auto minmax(0,1fr) auto"
          className="border-b-0 py-[13px]"
        >
          <Input
            autoFocus={autoFocus}
            className={`${rowInput} text-left font-semibold text-brand`}
            placeholder="my email"
            aria-label="Phrase to say"
            value={item.trigger}
            onChange={(e) => set({ ...item, trigger: e.target.value })}
          />
          <span aria-hidden className="text-hint text-chevron">
            →
          </span>
          <Textarea
            className="min-h-0 resize-none border-0 bg-transparent p-0 text-ui text-ink-2 shadow-none focus-visible:ring-0 dark:bg-transparent"
            placeholder="you@example.com"
            aria-label="Replacement text"
            // Grows with the text: a multi-line expansion was clipped to its
            // first line with nothing on screen to say so.
            rows={Math.min(6, item.expansion.split("\n").length)}
            value={item.expansion}
            onChange={(e) => set({ ...item, expansion: e.target.value })}
          />
          <DeleteRow onClick={remove} />
        </GridRow>
      )}
    >
      {children}
    </EditableList>
  );
}
