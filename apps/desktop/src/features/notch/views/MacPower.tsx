import { commands } from "@/lib/bindings";
import { Switch } from "@/components/notch/controls";
import { Empty, Label, Lead, List, Row, Value } from "@/components/notch/list";
import { Bar } from "@/components/notch/meters";
import { usePoll } from "../usePoll";
import { useWatcher } from "../useWatcher";
import { batteryTone, chargeSentence } from "./mac-format";

/** The estimate moves in minutes; ten seconds keeps a plug-in noticed promptly. */
const POLL_MS = 10_000;

/**
 * Mac › Power: the charge leads, with what it is doing and for how long; cycle
 * count and capacity health sit below, and the one switch is the watcher that
 * raises the low-battery and fully-charged islands.
 */
export function MacPower() {
  // `null` is a Mac with no battery; `undefined` is not read yet.
  const battery = usePoll(() => commands.macBattery(), POLL_MS);
  const [alerts, setAlerts] = useWatcher("battery");

  if (battery === undefined) {
    return <Empty title="Reading the battery">The charge shows up here a moment after the panel opens.</Empty>;
  }
  if (battery === null) {
    return (
      <Empty title="No battery in this Mac">
        A desktop Mac has none to read. This view fills in on a MacBook.
      </Empty>
    );
  }

  return (
    <div className="grid grid-cols-[1fr_1.4fr] gap-6 font-notch">
      <div>
        <Label>Battery</Label>
        <Lead sub={chargeSentence(battery.state, battery.minutes)}>{battery.percent}%</Lead>
        <Bar
          className="mt-2.5"
          value={battery.percent / 100}
          tone={batteryTone(battery.percent, battery.state)}
          label="Battery charge"
        />
      </div>
      <List label="Battery details">
        {battery.cycles != null && (
          <Row title="Charge cycles" subtitle="Full charges since it was new" trailing={<Value>{battery.cycles}</Value>} />
        )}
        {battery.health != null && (
          <Row title="Capacity" subtitle="Compared with when it was new" trailing={<Value>{battery.health}%</Value>} />
        )}
        <Row
          title="Alert me"
          subtitle="On a full charge, and beside the camera below 20%"
          trailing={
            <Switch
              checked={alerts ?? true}
              disabled={alerts === null}
              onChange={setAlerts}
              label="Battery alerts"
            />
          }
        />
      </List>
    </div>
  );
}
