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
    <div className="flex flex-col gap-4">
      <div className="flex items-start gap-4">
        <p className="flex-1 text-sm text-muted-foreground leading-relaxed">
          Adjust dictation cleanup per app: paste verbatim in terminals, or steer
          the tone in chat and email apps.
        </p>
        <Button variant="secondary" size="sm" onClick={() => setEditing(-1)}>
          Add Rule
        </Button>
      </div>

      {rules.length === 0 ? (
        <div className="py-8 text-center text-sm text-muted-foreground">
          <p className="mb-1 text-base font-semibold text-foreground">
            No App Rules Yet
          </p>
          <p>Click "Add Rule" to define your first app rule.</p>
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border border-border">
          {rules.map((r, i) => (
            <div
              key={i}
              className="flex items-center gap-4 border-b border-border/60 bg-muted/40 p-3 last:border-b-0"
            >
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="font-semibold">{r.app_matches}</span>
                <span className="truncate text-sm text-muted-foreground">
                  {summary(r)}
                </span>
              </div>
              <Button variant="link" size="sm" onClick={() => setEditing(i)}>
                Edit
              </Button>
              <Button
                variant="link"
                size="sm"
                className="text-destructive"
                onClick={() => remove(i)}
              >
                Delete
              </Button>
            </div>
          ))}
        </div>
      )}

      {suggestions.length > 0 && (
        <>
          <p className="text-xs font-semibold text-muted-foreground">
            Suggestions
          </p>
          <div className="overflow-hidden rounded-xl border border-border">
            {suggestions.map((s) => (
              <div
                key={s.app_matches}
                className="flex items-center gap-4 border-b border-border/60 bg-muted/20 p-3 last:border-b-0"
              >
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="font-semibold">{s.app_matches}</span>
                  <span className="truncate text-sm text-muted-foreground">
                    {summary(s)}
                  </span>
                </div>
                <Button
                  variant="link"
                  size="sm"
                  onClick={() => onChange([...rules, s])}
                >
                  Add
                </Button>
              </div>
            ))}
          </div>
        </>
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
