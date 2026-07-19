import { useEffect, useState } from "react";
import { commands, type AppRule, type InstalledApp } from "../../lib/bindings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Group } from "@/components/ui/rows";

/**
 * Per-app rules, modelled on the Swift app. A rule pins how dictation behaves
 * in one app: paste verbatim (no LLM cleanup) or steer the cleanup with a style
 * hint. Rules are added/edited in a shadcn Dialog; the app is chosen from a
 * searchable Command list of installed apps, and the search box doubles as a
 * free-text field so a rule can match any app-name substring, running or not.
 *
 * The two modes map onto the Rust `AppRule` fields:
 *  - Verbatim    → post_processing_enabled: false, custom_prompt: ""
 *  - Style hint  → post_processing_enabled: true,  custom_prompt: <hint>
 */

const isVerbatim = (r: AppRule) => r.post_processing_enabled === false;

function summary(r: AppRule): string {
  return isVerbatim(r)
    ? "Verbatim — no LLM cleanup"
    : `Style: ${r.custom_prompt || "…"}`;
}

// Starting points, never auto-applied. Mirrors AppRule.suggestions in Swift.
const SUGGESTIONS: AppRule[] = [
  { app_matches: "Terminal", post_processing_enabled: false, custom_prompt: "" },
  {
    app_matches: "Slack",
    post_processing_enabled: true,
    custom_prompt: "Keep the tone casual and brief.",
  },
  {
    app_matches: "Mail",
    post_processing_enabled: true,
    custom_prompt: "Polished, professional prose.",
  },
];

export function AppRuleEditor({
  rules,
  onChange,
}: {
  rules: AppRule[];
  onChange: (rules: AppRule[]) => void;
}) {
  const [apps, setApps] = useState<InstalledApp[]>([]);
  // null = closed; number = editing that index; -1 = adding a new rule.
  const [editing, setEditing] = useState<number | null>(null);

  useEffect(() => {
    commands.installedApps().then(setApps).catch(() => setApps([]));
  }, []);

  const taken = new Set(rules.map((r) => r.app_matches.toLowerCase()));
  const suggestions = SUGGESTIONS.filter(
    (s) => !taken.has(s.app_matches.toLowerCase()),
  );

  function save(rule: AppRule) {
    if (editing === -1) onChange([...rules, rule]);
    else onChange(rules.map((r, i) => (i === editing ? rule : r)));
    setEditing(null);
  }
  function remove(i: number) {
    onChange(rules.filter((_, j) => j !== i));
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-2xl font-bold leading-[1.1] tracking-[-0.02em]">
            App Rules
          </h1>
          <button
            className="shrink-0 whitespace-nowrap rounded-lg bg-[var(--accent)] px-[15px] py-[7px] text-[13px] font-medium text-white shadow-[0_1px_2px_rgba(0,60,160,.3)] active:scale-[0.97]"
            onClick={() => setEditing(-1)}
          >
            Add Rule
          </button>
        </div>
        <p className="mt-1.5 max-w-[520px] text-[13px] text-[var(--text2)]">
          Adjust dictation cleanup per app: paste verbatim in terminals, or steer
          the tone in chat and email apps.
        </p>
      </div>

      <Group>
        {rules.length === 0 ? (
          <div className="py-8 text-center text-[13px] text-[var(--text2)]">
            <p className="mb-1 text-[15px] font-semibold text-foreground">
              No App Rules Yet
            </p>
            <p>Click "Add Rule" to define your first app rule.</p>
          </div>
        ) : (
          rules.map((r, i) => (
            <div
              key={i}
              className="grid grid-cols-[130px_1fr_auto] items-center gap-3.5 border-b-[0.5px] border-[var(--sep)] py-[13px] last:border-b-0"
            >
              <span className="truncate text-[13.5px] font-semibold">
                {r.app_matches}
              </span>
              <span className="truncate text-[12.5px] text-[var(--text2)]">
                {summary(r)}
              </span>
              <div className="flex gap-4 text-[13px] font-medium">
                <button
                  className="text-[var(--accent)] active:scale-95"
                  onClick={() => setEditing(i)}
                >
                  Edit
                </button>
                <button
                  className="text-[var(--danger)] active:scale-95"
                  onClick={() => remove(i)}
                >
                  Delete
                </button>
              </div>
            </div>
          ))
        )}
      </Group>

      {suggestions.length > 0 && (
        <Group title="Suggestions">
          {suggestions.map((s) => (
            <div
              key={s.app_matches}
              className="grid grid-cols-[130px_1fr_auto] items-center gap-3.5 border-b-[0.5px] border-[var(--sep)] py-3 last:border-b-0"
            >
              <span className="truncate text-[13.5px] font-medium">
                {s.app_matches}
              </span>
              <span className="truncate text-[12.5px] text-[var(--text2)]">
                {summary(s)}
              </span>
              <button
                className="text-[13px] font-medium text-[var(--accent)] active:scale-95"
                onClick={() => onChange([...rules, s])}
              >
                Add
              </button>
            </div>
          ))}
        </Group>
      )}

      <RuleDialog
        apps={apps}
        open={editing !== null}
        rule={editing !== null && editing >= 0 ? rules[editing] : null}
        onOpenChange={(o) => !o && setEditing(null)}
        onSave={save}
      />
    </div>
  );
}

/** Add/Edit dialog: searchable installed-app picker + Verbatim/Style radios. */
function RuleDialog({
  apps,
  open,
  rule,
  onOpenChange,
  onSave,
}: {
  apps: InstalledApp[];
  open: boolean;
  rule: AppRule | null;
  onOpenChange: (open: boolean) => void;
  onSave: (rule: AppRule) => void;
}) {
  const [match, setMatch] = useState("");
  const [verbatim, setVerbatim] = useState(true);
  const [hint, setHint] = useState("");

  // Reset the form each time the dialog opens for a (possibly different) rule.
  useEffect(() => {
    if (!open) return;
    setMatch(rule?.app_matches ?? "");
    setVerbatim(rule ? isVerbatim(rule) : true);
    setHint(rule?.custom_prompt ?? "");
  }, [open, rule]);

  const picked = apps.find((a) => a.name === match);
  const canSave = match.trim() !== "" && (verbatim || hint.trim() !== "");

  function submit() {
    if (!canSave) return;
    onSave({
      app_matches: match.trim(),
      post_processing_enabled: !verbatim,
      custom_prompt: verbatim ? "" : hint.trim(),
    });
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="text-center">
            {rule ? "Edit App Rule" : "Add App Rule"}
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label>App</Label>
            <Command
              // Let free text match too: the typed value is a valid substring
              // rule even when it isn't one of the listed apps.
              className="rounded-lg border border-input"
              shouldFilter
            >
              <CommandInput
                value={match}
                onValueChange={setMatch}
                placeholder="Search apps, or type a name to match…"
              />
              <CommandList>
                <CommandEmpty>
                  Match "{match}" as a custom app name.
                </CommandEmpty>
                {apps.map((a) => (
                  <CommandItem
                    key={a.name}
                    value={a.name}
                    onSelect={() => setMatch(a.name)}
                  >
                    {a.icon ? (
                      <img
                        src={a.icon}
                        alt=""
                        className="size-5 rounded object-contain"
                      />
                    ) : (
                      <span className="size-5 rounded bg-muted" />
                    )}
                    {a.name}
                  </CommandItem>
                ))}
              </CommandList>
            </Command>
            {picked && (
              <p className="flex items-center gap-2 text-xs text-muted-foreground">
                {picked.icon && (
                  <img src={picked.icon} alt="" className="size-4 rounded" />
                )}
                Selected: {picked.name}
              </p>
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Label>Mode</Label>
            <RadioGroup
              value={verbatim ? "verbatim" : "style"}
              onValueChange={(v) => setVerbatim(v === "verbatim")}
            >
              <label className="flex items-center gap-2 text-sm">
                <RadioGroupItem value="verbatim" />
                Verbatim (no LLM cleanup)
              </label>
              <label className="flex items-center gap-2 text-sm">
                <RadioGroupItem value="style" />
                Style hint
              </label>
            </RadioGroup>
          </div>

          {!verbatim && (
            <Input
              placeholder="e.g. Keep the tone casual and brief"
              value={hint}
              onChange={(e) => setHint(e.target.value)}
            />
          )}
        </div>

        <DialogFooter className="justify-between sm:justify-between">
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!canSave} onClick={submit}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
