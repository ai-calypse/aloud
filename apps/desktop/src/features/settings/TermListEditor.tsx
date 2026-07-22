import { type ReactNode } from "react";
import { Input } from "@/components/ui/input";
import { GridRow, rowInput } from "@/components/ui/rows";
import { DeleteRow, EditableList } from "@/components/ui/editable-list";

/**
 * A list of words or phrases, one per row. Two of the Dictionary's three
 * mechanisms are this shape — vocabulary (bias recognition) and keep-exactly
 * (exempt from cleanup) — and differ only in copy.
 *
 * One row per term rather than a comma-separated field: a phrase like "I am um
 * boss" contains no comma but plenty of ambiguity about where it ends, and the
 * old single field silently split anything the user typed a comma inside.
 */
export function TermListEditor({
  title,
  terms,
  placeholder,
  addLabel,
  empty,
  onChange,
  children,
}: {
  title: string;
  terms: string[];
  placeholder: string;
  addLabel: string;
  empty: { title: string; body: string };
  onChange: (terms: string[]) => void;
  children?: ReactNode;
}) {
  return (
    <EditableList
      title={title}
      items={terms}
      onChange={onChange}
      make={() => ""}
      isBlank={(t) => !t.trim()}
      problem={(t, i) => {
        if (!t.trim()) return "Empty — this line does nothing.";
        // Folded the way both consumers compare: the transcriber's bias prompt
        // and the cleanup prompt are both case-blind in practice.
        const key = t.trim().toLowerCase();
        return terms.findIndex((o) => o.trim().toLowerCase() === key) !== i
          ? "Already listed above."
          : null;
      }}
      addLabel={addLabel}
      empty={empty}
      row={({ item, autoFocus, set, remove }) => (
        <GridRow columns="minmax(0,1fr) auto" className="border-b-0 py-[13px]">
          <Input
            autoFocus={autoFocus}
            className={`${rowInput} text-left`}
            placeholder={placeholder}
            aria-label={title}
            value={item}
            onChange={(e) => set(e.target.value)}
          />
          <DeleteRow onClick={remove} />
        </GridRow>
      )}
    >
      {children}
    </EditableList>
  );
}
