import { commands } from "@/lib/bindings";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/notch/controls";
import { Empty, Label, Lead, List, Row } from "@/components/notch/list";
import { usePoll } from "../usePoll";
import { useWatcher } from "../useWatcher";
import { privacyLead } from "./mac-format";

const POLL_MS = 1000;

/**
 * Mac › Privacy: whether the microphone or camera is on, and for the mic,
 * which app. The same reading drives the island beside the camera while the
 * notch is closed; this view is its detail and its off switch.
 */
export function MacPrivacy() {
  const use = usePoll(() => commands.macPrivacy(), POLL_MS);
  const [watching, setWatching] = useWatcher("privacy");

  if (!use) {
    return <Empty title="Checking the mic and camera">What is using them shows up here a moment after the panel opens.</Empty>;
  }

  const apps = use.micApps.join(", ");
  return (
    <div className="grid grid-cols-[1fr_1.4fr] gap-6 font-notch">
      <div>
        <Label>Mic and camera</Label>
        <Lead sub={use.mic || use.camera ? undefined : "Nothing is listening or watching"}>
          {privacyLead(use.mic, use.camera)}
        </Lead>
      </div>
      <List label="Mic and camera">
        <Row
          title="Microphone"
          subtitle={use.mic ? apps || "An app is recording" : "Not in use"}
          trailing={<Badge variant={use.mic ? "notch-warn" : "notch"}>{use.mic ? "In use" : "Off"}</Badge>}
        />
        <Row
          title="Camera"
          // macOS publishes whether a camera runs, never which app runs it.
          subtitle={use.camera ? "An app is using it; macOS does not say which" : "Not in use"}
          trailing={<Badge variant={use.camera ? "notch-warn" : "notch"}>{use.camera ? "In use" : "Off"}</Badge>}
        />
        <Row
          title="Show beside the camera"
          subtitle="An indicator while the notch is closed"
          trailing={
            <Switch
              checked={watching ?? true}
              disabled={watching === null}
              onChange={setWatching}
              label="Show mic and camera use"
            />
          }
        />
      </List>
    </div>
  );
}
