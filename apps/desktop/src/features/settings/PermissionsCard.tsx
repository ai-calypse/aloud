import { useCallback, useEffect, useState } from "react";
import {
  commands,
  events,
  type Permission,
  type PermissionStatus,
} from "../../lib/bindings";
import { or } from "../../lib/result";
import { Group, Row } from "@/components/ui/rows";
import { Button } from "@/components/ui/button";

/** What each permission is *for*, in terms of what stops working without it. */
const COPY: Record<Permission, { label: string; hint: string }> = {
  accessibility: {
    label: "Accessibility",
    hint: "Global hotkeys, pasting, and reading the selected text. Nothing works without it.",
  },
  microphone: {
    label: "Microphone",
    hint: "Recording. Denied, every dictation transcribes silence.",
  },
  calendar: {
    label: "Calendar",
    hint: "Today's events in the notch. Read only; Aloud never adds or changes an event.",
  },
  reminders: {
    label: "Reminders",
    hint: "Today's reminders in the notch. Ticking one off is the only change Aloud makes.",
  },
};

/**
 * Live status for every OS permission. Renders nothing on an empty list, which
 * is what Windows returns — no TCC, no pane to open.
 *
 * Refreshed on window focus rather than a timer: granting means leaving for
 * System Settings and coming back, so the return trip is the signal. The
 * `hotkeysPermitted` push covers the accessibility grant, which the waiting
 * hotkey thread sees without a focus change.
 */
export function PermissionsCard() {
  const [statuses, setStatuses] = useState<PermissionStatus[]>([]);

  const refresh = useCallback(() => {
    // Returns a bare value, not a Result, so `or` does not apply — but a
    // transport rejection still needs somewhere to land.
    commands
      .permissions()
      .then(setStatuses)
      .catch((e) => console.error(`reading permissions: ${e}`));
  }, []);

  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    const unlisten = events.hotkeysPermitted.listen(refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((off) => off());
    };
  }, [refresh]);

  if (statuses.length === 0) return null;

  return (
    <Group title="System access">
      {statuses.map(({ permission, granted }) => (
        <Row
          key={permission}
          label={COPY[permission].label}
          hint={COPY[permission].hint}
        >
          {granted ? (
            <span className="shrink-0 text-ui font-medium text-success">
              Granted
            </span>
          ) : (
            <Button
              variant="plain"
              size="text"
              onClick={() =>
                or(
                  commands.openPermissionSettings(permission),
                  null,
                  "opening System Settings",
                )
              }
            >
              Grant…
            </Button>
          )}
        </Row>
      ))}
    </Group>
  );
}
