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
  screenRecording: {
    label: "Screen Recording",
    hint: "Screen context. Nothing uses it yet — grant it when that ships.",
  },
};

/**
 * Live status for every OS permission, with a button to the right System
 * Settings pane.
 *
 * Renders nothing when the backend returns an empty list, which is what
 * Windows does: there is no TCC there and no pane to open.
 *
 * **Refreshed on window focus, not on a timer.** Granting a permission means
 * leaving for System Settings and coming back, so the return trip is the
 * signal — and it is one macOS already sends. A poll would burn a TCC lookup a
 * second forever to catch a change that only happens while the app is in the
 * background. The accessibility row additionally listens to the existing
 * `hotkeysPermitted` push, which fires without a focus change because the
 * hotkey thread is already waiting on that grant.
 */
export function PermissionsCard() {
  const [statuses, setStatuses] = useState<PermissionStatus[]>([]);

  const refresh = useCallback(() => {
    commands.permissions().then(setStatuses);
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
    <Group title="Permissions">
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
