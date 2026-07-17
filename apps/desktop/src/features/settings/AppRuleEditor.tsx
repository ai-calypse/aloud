import { type AppRule } from "../../lib/bindings";

/**
 * Edits per-app rules: match a focused app by name substring and override
 * cleanup on/off and the prompt for it. A blank prompt or "inherit" leaves the
 * global setting.
 */
export function AppRuleEditor({
  rules,
  onChange,
}: {
  rules: AppRule[];
  onChange: (rules: AppRule[]) => void;
}) {
  function set(i: number, patch: Partial<AppRule>) {
    onChange(rules.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  }
  function add() {
    onChange([
      ...rules,
      { app_matches: "", post_processing_enabled: null, custom_prompt: "" },
    ]);
  }
  function remove(i: number) {
    onChange(rules.filter((_, j) => j !== i));
  }

  // The tri-state override maps to a <select>: inherit / on / off.
  function cleanupValue(v: boolean | null): string {
    return v === null ? "inherit" : v ? "on" : "off";
  }
  function parseCleanup(s: string): boolean | null {
    return s === "inherit" ? null : s === "on";
  }

  return (
    <div className="listedit">
      {rules.length === 0 && (
        <p className="listedit__empty">
          No rules. Add one to change behaviour when a given app has focus.
        </p>
      )}
      {rules.map((r, i) => (
        <div key={i} className="listedit__rule">
          <div className="listedit__row">
            <input
              className="listedit__key"
              placeholder="app name contains… (e.g. slack)"
              value={r.app_matches}
              onChange={(e) => set(i, { app_matches: e.target.value })}
            />
            <select
              className="listedit__cleanup"
              value={cleanupValue(r.post_processing_enabled ?? null)}
              onChange={(e) =>
                set(i, { post_processing_enabled: parseCleanup(e.target.value) })
              }
            >
              <option value="inherit">Cleanup: inherit</option>
              <option value="on">Cleanup: on</option>
              <option value="off">Cleanup: off</option>
            </select>
            <button className="listedit__del" onClick={() => remove(i)} title="Remove">
              ✕
            </button>
          </div>
          <textarea
            className="listedit__prompt"
            placeholder="custom prompt for this app (optional)"
            rows={2}
            value={r.custom_prompt}
            onChange={(e) => set(i, { custom_prompt: e.target.value })}
          />
        </div>
      ))}
      <button className="listedit__add" onClick={add}>
        + Add rule
      </button>
    </div>
  );
}
