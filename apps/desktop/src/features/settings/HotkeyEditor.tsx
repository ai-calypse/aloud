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
import { cn } from "@/lib/utils";
import { describeBinding } from "./binding-label";

/**
 * One row per role, each bound by recording a real key gesture.
 *
 * This used to be a <select> of six modifier-only holds, which meant
 * `Binding::Key` — a normal key with modifiers — existed in Rust and could not
 * be reached from the app; anything else showed as "Custom (edit file)".
 * Recording happens in the Rust event tap rather than from DOM keydowns: the
 * browser cannot see a modifier-only press reliably, cannot tell Right Option
 * from Left, never sees Fn at all, and could not stop the hotkey being
 * *replaced* from firing while you record over it.
 *
 * The Config keys are snake_case (copy_again, meeting_notes) because they come
 * straight from the Rust struct, unlike the camelCase Settings fields.
 */

/** The roles a user binds, and the Config key each writes to. */
const ROLES: { key: keyof Config; label: string; mode: string; hint: string }[] = [
  { key: "hold", label: "Dictate", mode: "hold", hint: "Hold to talk. With text selected, rewrites it instead" },
  { key: "toggle", label: "Dictate", mode: "tap", hint: "Tap to start, tap again to stop" },
  { key: "ask", label: "Ask", mode: "hold", hint: "Hold, ask a question, read the answer" },
  { key: "rewrite", label: "Rewrite", mode: "hold", hint: "Hold to change the selected text by voice" },
  { key: "agent", label: "Agent", mode: "hold", hint: "Hold to run a command across apps" },
  { key: "meeting_notes", label: "Meeting notes", mode: "tap", hint: "Tap to start/stop a session" },
  { key: "copy_again", label: "Paste again", mode: "tap", hint: "Re-paste the last transcript" },
];

const DISABLED: Binding = { kind: "disabled" };

/**
 * Whether another role is bound to the same gesture.
 *
 * Advisory only — `matcher.rs` orders simultaneous activations by specificity,
 * so a clash is resolved rather than broken. But two roles on one key is almost
 * always a mistake, and silently letting the more specific one win is how you
 * end up with a hotkey that "stopped working".
 */
function conflictsWith(config: Config, role: keyof Config): boolean {
  const mine = config[role];
  if (!mine || mine.kind === "disabled") return false;
  return ROLES.some((other) => {
    if (other.key === role) return false;
    const theirs = config[other.key];
    if (!theirs || theirs.kind === "disabled") return false;
    return (
      theirs.kind === mine.kind &&
      sameKey(theirs.key, mine.key) &&
      sameModifiers(theirs.modifiers, mine.modifiers)
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

function sameModifiers(a: Modifier[], b: Modifier[]): boolean {
  return a.length === b.length && a.every((m) => b.includes(m));
}

export function HotkeyEditor({
  config,
  onChange,
}: {
  config: Config;
  onChange: (config: Config) => void;
}) {
  /** Which role is recording, if any. Only one at a time. */
  const [recording, setRecording] = useState<keyof Config | null>(null);
  // Rust times capture out on its own, so an abandoned recording cannot hold
  // the keyboard. This ref is only to ignore a result that arrives after the
  // user has already moved on to recording a different role.
  const generation = useRef(0);

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
    if (result.data) onChange({ ...config, [role]: result.data });
  }

  function cancel() {
    generation.current++;
    setRecording(null);
    commands.cancelHotkeyCapture();
  }

  return (
    <Group>
      {ROLES.map((role) => {
        const binding = config[role.key] ?? DISABLED;
        const isRecording = recording === role.key;
        const bound = binding.kind !== "disabled";
        const clash = conflictsWith(config, role.key);

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
                  Same key as another role — the more specific one wins
                </span>
              ) : (
                role.hint
              )}
            </span>

            <div className="flex items-center gap-2.5">
              {/* Styled as a keycap: the binding is the value of the row, so it
                  should read as a key rather than as a form field. */}
              <button
                type="button"
                onClick={() => (isRecording ? cancel() : record(role.key))}
                className={cn(
                  "rounded-row border-[0.5px] px-3 py-[5px] font-mono text-xs",
                  "transition-colors duration-[var(--dur-fast)]",
                  isRecording
                    ? "animate-pulse border-brand bg-brand/10 text-brand"
                    : "border-keycap-border bg-keycap hover:border-brand",
                  !bound && !isRecording && "text-ink-3",
                )}
              >
                {isRecording ? "Press keys…" : describeBinding(binding)}
              </button>

              {isRecording ? (
                <Button variant="plain" size="text" onClick={cancel}>
                  Cancel
                </Button>
              ) : (
                // Reserves the column so keycaps stay aligned down the list.
                <Button
                  variant="plain"
                  size="text"
                  className={cn(!bound && "invisible")}
                  onClick={() => onChange({ ...config, [role.key]: DISABLED })}
                >
                  Clear
                </Button>
              )}
            </div>
          </GridRow>
        );
      })}
    </Group>
  );
}
