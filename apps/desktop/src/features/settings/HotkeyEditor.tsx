import { type Binding, type Config, type Key } from "../../lib/bindings";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/**
 * A compact hotkey binder, one row per role. A full keystroke recorder is the
 * eventual upgrade; this offers the modifier-only holds that actually work on
 * both platforms (the ones `Config::defaults` and the Swift app used), which is
 * enough to bind and test every mode. "Disabled" turns a role off.
 *
 * The Config keys are snake_case (copy_again, meeting_notes) because they come
 * straight from the Rust struct, unlike the camelCase Settings fields.
 */

/** The roles a user binds, and the Config key each writes to. */
const ROLES: { key: keyof Config; label: string; hint: string }[] = [
  { key: "hold", label: "Dictate (hold)", hint: "Hold to talk, release to paste" },
  { key: "ask", label: "Ask (hold)", hint: "Hold, ask a question, read the answer" },
  { key: "command", label: "Command (hold)", hint: "Hold to transform the selection" },
  { key: "agent", label: "Agent (hold)", hint: "Hold to run a command across apps" },
  { key: "meeting_notes", label: "Meeting notes", hint: "Tap to start/stop a session" },
  { key: "copy_again", label: "Paste again", hint: "Re-paste the last transcript" },
];

/** The modifier-only holds offered, keyed by a stable id for the <select>. */
const CHOICES: { id: string; label: string; binding: Binding }[] = [
  { id: "disabled", label: "Off", binding: { kind: "disabled" } },
  { id: "rightOption", label: "Right Option", binding: modKey("rightOption") },
  { id: "leftOption", label: "Left Option", binding: modKey("leftOption") },
  { id: "rightCommand", label: "Right Command", binding: modKey("rightCommand") },
  { id: "rightControl", label: "Right Control", binding: modKey("rightControl") },
  { id: "rightShift", label: "Right Shift", binding: modKey("rightShift") },
  { id: "fn", label: "Fn", binding: modKey("fn") },
];

function modKey(key: Key): Binding {
  return { kind: "modifierKey", key, modifiers: [] };
}

/** Maps a stored binding back to a choice id, so the <select> shows it. */
function choiceId(binding: Binding | null): string {
  if (!binding || binding.kind === "disabled") return "disabled";
  if (binding.kind === "modifierKey") {
    const hit = CHOICES.find(
      (c) => c.binding.kind === "modifierKey" && c.binding.key === binding.key,
    );
    if (hit) return hit.id;
  }
  // A key-combo binding set elsewhere (e.g. hand-edited) has no preset; show it
  // as a custom row rather than silently resetting it.
  return "custom";
}

export function HotkeyEditor({
  config,
  onChange,
}: {
  config: Config;
  onChange: (config: Config) => void;
}) {
  function setRole(role: keyof Config, id: string) {
    const choice = CHOICES.find((c) => c.id === id);
    if (!choice) return;
    onChange({ ...config, [role]: choice.binding });
  }

  return (
    <div className="flex flex-col gap-3">
      {ROLES.map((role) => {
        const current = choiceId(config[role.key]);
        return (
          <div
            key={role.key}
            className="grid grid-cols-[9rem_1fr] items-center gap-x-4 gap-y-1 text-sm"
          >
            <span>{role.label}</span>
            <Select
              value={current === "custom" ? "custom" : current}
              onValueChange={(v) => setRole(role.key, v)}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {current === "custom" && (
                  <SelectItem value="custom">Custom (edit file)</SelectItem>
                )}
                {CHOICES.map((c) => (
                  <SelectItem key={c.id} value={c.id}>
                    {c.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <small className="col-start-2 text-xs text-muted-foreground">
              {role.hint}
            </small>
          </div>
        );
      })}
      <p className="text-xs text-muted-foreground">
        Two roles on the same key will clash — give each its own.
      </p>
    </div>
  );
}
