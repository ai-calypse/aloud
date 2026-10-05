import { useEffect, useState } from "react";
import {
  commands,
  events,
  type InstalledApp,
  type MirrorState,
  type NotchEvents,
} from "@/lib/bindings";
import { or } from "@/lib/result";
import { Row, Toggle, Value } from "@/components/ui/rows";
import { Button } from "@/components/ui/button";

/** What the watcher's state says to the person, and whether it needs them. */
function say(state: MirrorState): { text: string; needsYou: boolean } {
  switch (state.kind) {
    case "watching":
      return { text: "Watching Notification Center", needsYou: false };
    case "needsAccess":
      return {
        text: "macOS is not letting Aloud read Notification Center. Turn Aloud on under Full Disk Access, then check again.",
        needsYou: true,
      };
    case "unsupported":
      return { text: `This macOS keeps Notification Center in a form Aloud cannot read (${state.detail}).`, needsYou: false };
    case "off":
      return { text: "Starting…", needsYou: false };
  }
}

/**
 * Settings › Overlay: mirror other apps' notifications in the notch. Opt-in,
 * because it reads Notification Center's database (Full Disk Access) and shows
 * what other apps say. Only the apps allowed here are drawn.
 */
export function NudgeSettings({
  value,
  onChange,
}: {
  value: Required<NotchEvents>;
  onChange: (next: NotchEvents) => void;
}) {
  const [state, setState] = useState<MirrorState>({ kind: "off" });
  const [apps, setApps] = useState<InstalledApp[]>([]);
  const on = value.appNotifications;

  useEffect(() => {
    void commands.nudgeState().then(setState);
    const off = events.nudgeState.listen((e) => setState(e.payload.state));
    return () => void off.then((f) => f());
  }, []);
  useEffect(() => {
    or(commands.installedApps(), [], "listing installed apps").then(setApps);
  }, []);

  const nameOf = (bundle: string) =>
    apps.find((a) => a.bundle_id === bundle)?.name ?? bundle.split(".").pop() ?? bundle;
  const allowed = value.appNotificationAllow;
  const addable = apps.filter((a) => a.bundle_id && !allowed.includes(a.bundle_id));
  const set = (patch: Partial<NotchEvents>) => onChange({ ...value, ...patch });
  const status = say(state);

  return (
    <>
      <Row
        label="Show other apps' notifications in the notch"
        hint="Messages, Mail, Slack, a FaceTime call and the apps you allow below appear in the notch as well as in Notification Center. Aloud reads Notification Center's database on this Mac, which needs Full Disk Access. What a notification says is shown and then forgotten: it is never saved, logged or sent anywhere. Aloud cannot reply, answer a call or accept an AirDrop for you; the button opens the app."
        className="py-[11px]"
      >
        <Toggle
          checked={on}
          label="Show other apps' notifications in the notch"
          onChange={(c) => set({ appNotifications: c })}
        />
      </Row>
      {on && (
        <>
          <Row label="Full Disk Access" hint={status.text} className="py-[11px]">
            <div className="flex shrink-0 gap-3">
              {status.needsYou && (
                <Button variant="accent" size="row" onClick={() => void commands.nudgeOpenAccessSettings()}>
                  Grant Full Disk Access
                </Button>
              )}
              <Button variant="plain" size="text" onClick={() => void commands.nudgeRecheck()}>
                Check again
              </Button>
            </div>
          </Row>
          <Row
            label="Apps"
            hint={
              allowed.length === 0
                ? "None: nothing is mirrored until you add an app."
                : allowed.map(nameOf).join(", ")
            }
            className="py-[11px]"
          >
            <Value>
              <select
                className="max-w-[200px] bg-transparent text-row text-ink-2"
                aria-label="Add an app"
                value=""
                onChange={(e) => e.target.value && set({ appNotificationAllow: [...allowed, e.target.value] })}
              >
                <option value="">Add an app…</option>
                {addable.map((a) => (
                  <option key={a.bundle_id} value={a.bundle_id ?? ""}>
                    {a.name}
                  </option>
                ))}
              </select>
            </Value>
          </Row>
          {allowed.map((bundle) => (
            <Row key={bundle} label={nameOf(bundle)} hint={bundle} className="py-[9px]">
              <Button
                variant="plain-danger"
                size="text"
                onClick={() => set({ appNotificationAllow: allowed.filter((b) => b !== bundle) })}
              >
                Remove
              </Button>
            </Row>
          ))}
        </>
      )}
    </>
  );
}
