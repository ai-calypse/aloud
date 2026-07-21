import { useEffect, useState } from "react";
import { commands, type AppRule, type InstalledApp } from "../../lib/bindings";
import { or } from "../../lib/result";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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
import { GridRow, Group, Row, Surface, Toggle } from "@/components/ui/rows";

/**
 * Per-app rules, modelled on the Swift app. A rule pins how dictation behaves
 * in one app: paste verbatim (no LLM cleanup) or steer the cleanup with a style
 * hint. Rules are added/edited in a shadcn Dialog; the app is chosen from a
 * searchable Command list of installed apps, and the search box doubles as a
 * free-text field so a rule can match any app-name substring, running or not.
 *
 * The two `AppRule` fields are edited independently rather than collapsed into
 * a mode: a rule that cleans up *without* a style hint is a real combination
 * the model allows, and a single Verbatim/Style radio made it unreachable.
 */

const isVerbatim = (r: AppRule) => r.post_processing_enabled === false;

function summary(r: AppRule): string {
  if (isVerbatim(r)) return "Verbatim — no LLM cleanup";
  return r.custom_prompt ? `Style: ${r.custom_prompt}` : "Cleanup, no style hint";
}

// Starting points, never auto-applied. Mirrors AppRule.suggestions in Swift.
// No bundle ids here: a suggestion is a name the user may not even have
// installed, and guessing an id that turns out to be wrong would key the rule
// on something that can never match.
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
    // An empty list is a valid answer (Windows has no scan yet), so a failure
    // logs rather than silently looking like "no apps installed". The picker
    // still works: its search box doubles as a free-text app-name field.
    or(commands.installedApps(), [], "listing installed apps").then(setApps);
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
          <Button
            variant="accent"
            size="row"
            className="shrink-0 whitespace-nowrap"
            onClick={() => setEditing(-1)}
          >
            Add Rule
          </Button>
        </div>
        <p className="mt-1.5 max-w-[520px] text-ui text-ink-2">
          Adjust dictation cleanup per app: paste verbatim in terminals, or steer
          the tone in chat and email apps.
        </p>
      </div>

      <Group>
        {rules.length === 0 ? (
          <div className="py-8 text-center text-ui text-ink-2">
            <p className="mb-1 text-[15px] font-semibold text-foreground">
              No App Rules Yet
            </p>
            <p>Click "Add Rule" to define your first app rule.</p>
          </div>
        ) : (
          rules.map((r, i) => (
            <GridRow key={i} columns="130px 1fr auto" className="py-[13px]">
              <span className="truncate text-row font-semibold">
                {r.app_matches}
              </span>
              <span className="truncate text-hint text-ink-2">
                {summary(r)}
              </span>
              <div className="flex gap-4">
                <Button
                  variant="plain"
                  size="text"
                  onClick={() => setEditing(i)}
                >
                  Edit
                </Button>
                <Button
                  variant="plain-danger"
                  size="text"
                  onClick={() => remove(i)}
                >
                  Delete
                </Button>
              </div>
            </GridRow>
          ))
        )}
      </Group>

      {suggestions.length > 0 && (
        <Group title="Suggestions">
          {suggestions.map((s) => (
            <GridRow key={s.app_matches} columns="130px 1fr auto">
              <span className="truncate text-row font-medium">
                {s.app_matches}
              </span>
              <span className="truncate text-hint text-ink-2">
                {summary(s)}
              </span>
              <Button
                variant="plain"
                size="text"
                onClick={() => onChange([...rules, s])}
              >
                Add
              </Button>
            </GridRow>
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
  // Only the app matcher is required. A hint is optional now that cleanup is
  // its own switch — "clean up, no style hint" is a valid rule.
  const canSave = match.trim() !== "";

  function submit() {
    if (!canSave) return;
    onSave({
      app_matches: match.trim(),
      // Only when the name came from the picker: a bundle id is the exact
      // identity of *that* app, so attaching one to hand-typed free text would
      // key the rule on an app the user never chose. Typed rules keep matching
      // on the name, which is what they have always done.
      //
      // Falls back to the id the rule already had, and that fallback is the
      // whole point of the feature. `picked` is an exact name match against
      // the *currently installed* apps, recomputed on save — so editing a rule
      // for an app that has since been renamed (the exact case a bundle id
      // exists to survive) would find nothing and write null, destroying the
      // rename-survival property and dropping the rule back to matching a name
      // that no longer exists. Only an explicit change of the matcher text
      // clears it.
      bundle_id: picked?.bundle_id ?? rule?.bundle_id ?? null,
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
                {picked.bundle_id && (
                  <span className="truncate">· {picked.bundle_id}</span>
                )}
              </p>
            )}
          </div>

          <Surface className="px-[15px]">
            <Row
              label="Clean up with the LLM"
              hint="Off pastes the raw transcript verbatim."
            >
              <Toggle
                checked={!verbatim}
                onChange={(on) => setVerbatim(!on)}
                label="Clean up with the LLM"
              />
            </Row>
          </Surface>

          {!verbatim && (
            <div className="flex flex-col gap-1.5">
              <Label>Style hint (optional)</Label>
              <Input
                placeholder="e.g. Keep the tone casual and brief"
                value={hint}
                onChange={(e) => setHint(e.target.value)}
              />
            </div>
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
