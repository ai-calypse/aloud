import { type Macro } from "../../lib/bindings";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Group, rowInput } from "@/components/ui/rows";

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
          <div className="py-8 text-center text-[13px] text-[var(--text2)]">
            No macros. Add one and say its trigger while dictating to expand it.
          </div>
        ) : (
          macros.map((m, i) => (
            <div
              key={i}
              className="grid grid-cols-[200px_1fr_auto] items-center gap-3.5 border-b-[0.5px] border-[var(--sep)] py-[13px] last:border-b-0"
            >
              <Input
                className={`${rowInput} text-left font-semibold text-[var(--accent)]`}
                placeholder="trigger (e.g. my email)"
                value={m.trigger}
                onChange={(e) => set(i, { trigger: e.target.value })}
              />
              <Textarea
                className="min-h-0 resize-none border-0 bg-transparent p-0 text-[13px] text-[var(--text2)] shadow-none focus-visible:ring-0 dark:bg-transparent"
                placeholder="expands to…"
                rows={1}
                value={m.expansion}
                onChange={(e) => set(i, { expansion: e.target.value })}
              />
              <button
                className="text-[13px] font-medium text-[var(--danger)] active:scale-95"
                onClick={() => remove(i)}
              >
                Delete
              </button>
            </div>
          ))
        )}
      </Group>
      <button
        className="self-start rounded-lg bg-[var(--accent)] px-[15px] py-[7px] text-[13px] font-medium text-white shadow-[0_1px_2px_rgba(0,60,160,.3)] active:scale-[0.97]"
        onClick={add}
      >
        Add Macro
      </button>
    </div>
  );
}
