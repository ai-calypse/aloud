import { useEffect, useRef, useState } from "react";
import {
  commands,
  type Binding,
  type Config,
  type Key,
  type Modifier,
} from "../../lib/bindings";
import { GridRow, Group } from "@/components/ui/rows";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";
import { describeBinding } from "./binding-label";

/**
 * One row per role, each bound by recording a real key gesture.
 *
 * Recording happens in the Rust event tap, not from DOM keydowns: the browser
 * cannot see a modifier-only press reliably, cannot tell Right Option from
 * Left, never sees Fn, and cannot stop the hotkey being replaced from firing
 * while you record over it.
 *
 * Config keys are snake_case because they come straight from the Rust struct,
 * unlike the camelCase Settings fields.
 */

/** The roles a user binds, and the Config key each writes to. */
const ROLES: { key: keyof Config; label: string; mode: string; hint: string; agent?: true }[] = [
  { key: "hold", label: "Dictate", mode: "hold", hint: "Hold to talk. With text selected, rewrites it instead" },
  { key: "toggle", label: "Dictate", mode: "tap", hint: "Tap to start, tap again to stop" },
  { key: "ask", label: "Command (second key)", mode: "hold", hint: "Same as Command — was Ask", agent: true },
  { key: "rewrite", label: "Rewrite", mode: "hold", hint: "Hold to change the selected text by voice" },
  { key: "agent", label: "Command", mode: "hold", hint: "Hold, then ask anything or say what to do", agent: true },
  { key: "journal", label: "Capture thought", mode: "hold", hint: "Hold to speak a thought into your Journal" },
  { key: "meeting_notes", label: "Meeting notes", mode: "tap", hint: "Tap to start/stop a session" },
  { key: "copy_again", label: "Paste again", mode: "tap", hint: "Re-paste the last transcript" },
];

const DISABLED: Binding = { kind: "disabled" };

/** Mirrors Rust's `Config::defaults()`. Duplicated rather than fetched because
 * a reset must work without a round-trip; a Rust test keeps the two in step. */
const DEFAULT_CONFIG: Config = {
  hold: { kind: "modifierKey", key: "rightOption", modifiers: [] },
  toggle: null,
  copy_again: null,
  ask: null,
  rewrite: null,
  meeting_notes: null,
  agent: null,
  journal: null,
};

/**
 * One-click holds, all modifier-only so they fire on a bare hold. "Both
 * options" is absent because the matcher has no either/both-sides concept.
 * Option+Fn is keyed on Fn with Option *required*, so either Option side fires
 * it and the exact-match rule rejects any further modifier.
 */
const PRESETS: Binding[] = [
  { kind: "modifierKey", key: "fn", modifiers: [] },
  { kind: "modifierKey", key: "rightOption", modifiers: [] },
  { kind: "modifierKey", key: "leftOption", modifiers: [] },
  { kind: "modifierKey", key: "fn", modifiers: ["option"] },
];

/** Recorded gestures kept for the dropdown. A UI convenience, not a setting
 * the app acts on, so localStorage rather than a Config field and a migration. */
const CUSTOMS_KEY = "aloud.hotkey.customs";

function loadCustoms(): Binding[] {
  try {
    const raw = localStorage.getItem(CUSTOMS_KEY);
    return raw ? (JSON.parse(raw) as Binding[]) : [];
  } catch {
    return [];
  }
}

function isPreset(b: Binding): boolean {
  return PRESETS.some((p) => sameBinding(b, p));
}

/** Whether a role's current binding is exactly this preset, for the on-state. */
function sameBinding(a: Binding | undefined, b: Binding): boolean {
  if (!a || a.kind === "disabled" || b.kind === "disabled") return a?.kind === b.kind;
  return (
    a.kind === b.kind &&
    sameKey(a.key, b.key) &&
    a.modifiers.length === b.modifiers.length &&
    a.modifiers.every((m) => b.modifiers.includes(m)) &&
    // A side-specific chord is not its either-side twin.
    sameKeys(a.exact_modifiers ?? [], b.exact_modifiers ?? [])
  );
}

/**
 * Whether another role has the *identical* gesture. Only an exact match is a
 * clash: a strict overlap (`fn` vs `⌥fn`) is resolved by the pipeline, where
 * the more-specific role owns the session (`Session::owner`, `Pipeline::owns`).
 */
function conflictsWith(
  config: Config,
  role: keyof Config,
  roles: typeof ROLES,
): boolean {
  const mine = config[role];
  if (!mine || mine.kind === "disabled") return false;
  return roles.some((other) => {
    if (other.key === role) return false;
    const theirs = config[other.key];
    if (!theirs || theirs.kind === "disabled") return false;
    return (
      sameKey(theirs.key, mine.key) &&
      theirs.modifiers.length === mine.modifiers.length &&
      isSubset(theirs.modifiers, mine.modifiers) &&
      // Same key and modifiers but different sides is not a clash — Left-Option
      // and Right-Option chords never both fire.
      sameKeys(mine.exact_modifiers ?? [], theirs.exact_modifiers ?? [])
    );
  });
}

/** Key is a string union plus two object variants, so `===` is not enough. */
function sameKey(a: Key, b: Key): boolean {
  if (typeof a === "string" || typeof b === "string") return a === b;
  if ("char" in a && "char" in b) return a.char === b.char;
  if ("f" in a && "f" in b) return a.f === b.f;
  return false;
}

/** Two key sets equal ignoring order — for the side-specific exact-modifier set. */
function sameKeys(a: Key[], b: Key[]): boolean {
  return a.length === b.length && a.every((k) => b.some((k2) => sameKey(k, k2)));
}

function isSubset(a: Modifier[], b: Modifier[]): boolean {
  return a.every((m) => b.includes(m));
}

export function HotkeyEditor({
  config,
  onChange,
  voiceAgent = true,
}: {
  config: Config;
  onChange: (config: Config) => void;
  /** Settings › Labs. Off, the Command rows are hidden: Rust unbinds those
   * keys anyway (`labs::hotkeys`), and a row that does nothing reads as broken.
   * The saved bindings are kept, so turning the agent back on restores them. */
  voiceAgent?: boolean;
}) {
  const roles = voiceAgent ? ROLES : ROLES.filter((r) => !r.agent);
  const [recording, setRecording] = useState<keyof Config | null>(null);
  const [customs, setCustoms] = useState<Binding[]>(loadCustoms);
  // Ignores a capture that lands after the user moved on to another role.
  // (Rust times capture out on its own, so nothing can hold the keyboard.)
  const generation = useRef(0);
  // A ref, not the prop: capture takes up to ten seconds and other rows stay
  // editable, so spreading the `config` from the click would undo them.
  const latest = useRef(config);
  latest.current = config;

  // Recording swallows every key, so leaving the pane must not strand it.
  useEffect(() => {
    return () => {
      commands.cancelHotkeyCapture();
    };
  }, []);

  async function record(role: keyof Config) {
    const mine = ++generation.current;
    setRecording(role);

    const result = await commands.captureHotkey();
    if (generation.current !== mine) return;
    setRecording(null);

    if (result.status === "error") {
      console.error(`capturing hotkey: ${result.error}`);
      return;
    }
    // null is a timeout or a cancel — leave the existing binding alone.
    if (!result.data) return;
    const captured = result.data;
    if (captured.kind !== "disabled" && !isPreset(captured)) {
      setCustoms((prev) => {
        if (prev.some((c) => sameBinding(c, captured))) return prev;
        const next = [...prev, captured];
        try {
          localStorage.setItem(CUSTOMS_KEY, JSON.stringify(next));
        } catch {
          // A full/blocked store just means it won't persist — not fatal.
        }
        return next;
      });
    }
    onChange({ ...latest.current, [role]: captured });
  }

  function cancel() {
    generation.current++;
    setRecording(null);
    commands.cancelHotkeyCapture();
  }

  function reset() {
    cancel();
    try {
      localStorage.removeItem(CUSTOMS_KEY);
    } catch {
      // A blocked store just means the list survives; the bindings still reset.
    }
    setCustoms([]);
    onChange(DEFAULT_CONFIG);
  }

  return (
    <>
    <Group>
      {roles.map((role) => {
        const binding = config[role.key] ?? DISABLED;
        const isRecording = recording === role.key;
        const bound = binding.kind !== "disabled";
        const clash = conflictsWith(config, role.key, roles);

        // A hand-edited binding not in `customs` is added so it still renders
        // as the selected value rather than a blank trigger.
        const options = [...PRESETS, ...customs];
        if (bound && !options.some((o) => sameBinding(o, binding))) {
          options.push(binding);
        }

        // Options taken by another role drop out, so two commands cannot be
        // aimed at one hold. This role's own binding stays, to render as value.
        const takenElsewhere = new Set(
          roles.filter((r) => r.key !== role.key)
            .map((r) => config[r.key])
            .filter((b): b is Binding => !!b && b.kind !== "disabled")
            .map(describeBinding),
        );
        const menuOptions = options.filter(
          (o) =>
            sameBinding(binding, o) || !takenElsewhere.has(describeBinding(o)),
        );

        return (
          <GridRow key={role.key} columns="150px 1fr auto">
            <div className="whitespace-nowrap text-row font-medium">
              {role.label}
              <span className="ml-[7px] text-[10px] uppercase tracking-[0.06em] text-ink-3">
                {role.mode}
              </span>
            </div>
            <span className="text-hint text-ink-2">
              {clash ? (
                <span className="text-warning">
                  Same key as another role — both fire, and only one gets the
                  recording
                </span>
              ) : (
                role.hint
              )}
            </span>

            <div className="flex items-center gap-2.5">
              {isRecording ? (
                <>
                  <span className="animate-pulse rounded-row border-[0.5px] border-brand bg-brand/10 px-3 py-[5px] font-mono text-xs text-brand">
                    Press keys…
                  </span>
                  <Button variant="plain" size="text" onClick={cancel}>
                    Cancel
                  </Button>
                </>
              ) : (
                <>
                  <Select
                    value={bound ? describeBinding(binding) : ""}
                    onValueChange={(value) => {
                      const picked = options.find(
                        (o) => describeBinding(o) === value,
                      );
                      if (picked) onChange({ ...config, [role.key]: picked });
                    }}
                  >
                    <SelectTrigger
                      size="sm"
                      className="h-8 w-[176px] text-xs"
                    >
                      <SelectValue placeholder="Not set" />
                    </SelectTrigger>
                    <SelectContent align="end">
                      {menuOptions.map((o) => (
                        <SelectItem
                          key={describeBinding(o)}
                          value={describeBinding(o)}
                          className="text-xs"
                        >
                          {describeBinding(o)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  <Button
                    variant="plain"
                    size="text"
                    onClick={() => record(role.key)}
                  >
                    Record
                  </Button>
                  <Button
                    variant="plain"
                    size="text"
                    className={cn(!bound && "invisible")}
                    onClick={() => onChange({ ...config, [role.key]: DISABLED })}
                  >
                    Clear
                  </Button>
                </>
              )}
            </div>
          </GridRow>
        );
      })}
    </Group>
    <div className="flex justify-end px-1 pt-2">
      <Button variant="plain-danger" size="text" onClick={reset}>
        Reset to defaults
      </Button>
    </div>
    </>
  );
}
