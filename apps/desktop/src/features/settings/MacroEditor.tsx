import { type Macro } from "../../lib/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { GridRow, Group, rowInput } from "@/components/ui/rows";

/**
 * Edits the voice-macro list: a trigger phrase and the text it expands to.
 * Add/remove rows; each edit persists through the parent's `update`.
 */
export function MacroEditor({
  macros,
  onChange,
}: {
  macros: Macro[];
  onChange: (macros: Macro[]) => void;
}) {
  function set(i: number, patch: Partial<Macro>) {
    onChange(macros.map((m, j) => (j === i ? { ...m, ...patch } : m)));
  }
  function add() {
    onChange([...macros, { trigger: "", expansion: "" }]);
  }
  function remove(i: number) {
    onChange(macros.filter((_, j) => j !== i));
  }

  return (
    <div className="flex flex-col gap-4">
      <Group>
        {macros.length === 0 ? (
          <div className="py-8 text-center text-ui text-ink-2">
            No macros. Add one and say its trigger while dictating to expand it.
          </div>
        ) : (
          macros.map((m, i) => (
            <GridRow key={i} columns="200px 1fr auto" className="py-[13px]">
              <Input
                className={`${rowInput} text-left font-semibold text-brand`}
                placeholder="trigger (e.g. my email)"
                value={m.trigger}
                onChange={(e) => set(i, { trigger: e.target.value })}
              />
              <Textarea
                className="min-h-0 resize-none border-0 bg-transparent p-0 text-ui text-ink-2 shadow-none focus-visible:ring-0 dark:bg-transparent"
                placeholder="expands to…"
                rows={1}
                value={m.expansion}
                onChange={(e) => set(i, { expansion: e.target.value })}
              />
              <Button
                variant="plain-danger"
                size="text"
                onClick={() => remove(i)}
              >
                Delete
              </Button>
            </GridRow>
          ))
        )}
      </Group>
      <Button variant="accent" size="row" className="self-start" onClick={add}>
        Add Macro
      </Button>
    </div>
  );
}
