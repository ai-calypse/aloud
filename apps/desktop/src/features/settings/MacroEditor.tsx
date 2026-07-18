import { type Macro } from "../../lib/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

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
    <div className="flex flex-col gap-2.5">
      {macros.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No macros. Add one and say its trigger while dictating to expand it.
        </p>
      )}
      {macros.map((m, i) => (
        <div key={i} className="flex items-start gap-2">
          <Input
            className="basis-2/5"
            placeholder="trigger (e.g. my email)"
            value={m.trigger}
            onChange={(e) => set(i, { trigger: e.target.value })}
          />
          <Textarea
            className="flex-1 min-h-9"
            placeholder="expands to…"
            rows={1}
            value={m.expansion}
            onChange={(e) => set(i, { expansion: e.target.value })}
          />
          <Button
            variant="outline"
            size="icon"
            onClick={() => remove(i)}
            title="Remove"
          >
            ✕
          </Button>
        </div>
      ))}
      <Button variant="outline" size="sm" className="self-start" onClick={add}>
        + Add macro
      </Button>
    </div>
  );
}
