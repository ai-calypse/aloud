import { type Macro } from "../../lib/bindings";

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
    <div className="listedit">
      {macros.length === 0 && (
        <p className="listedit__empty">
          No macros. Add one and say its trigger while dictating to expand it.
        </p>
      )}
      {macros.map((m, i) => (
        <div key={i} className="listedit__row">
          <input
            className="listedit__key"
            placeholder="trigger (e.g. my email)"
            value={m.trigger}
            onChange={(e) => set(i, { trigger: e.target.value })}
          />
          <textarea
            className="listedit__val"
            placeholder="expands to…"
            rows={1}
            value={m.expansion}
            onChange={(e) => set(i, { expansion: e.target.value })}
          />
          <button className="listedit__del" onClick={() => remove(i)} title="Remove">
            ✕
          </button>
        </div>
      ))}
      <button className="listedit__add" onClick={add}>
        + Add macro
      </button>
    </div>
  );
}
