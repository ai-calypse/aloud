import { useState } from "react";
import { commands, type BtDevice } from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { AppIcon } from "@/components/notch/app-icon";
import { Empty, Label, Lead, List, Row, Value } from "@/components/notch/list";
import { Bar } from "@/components/notch/meters";
import { lowest, usePolled } from "../media";

type Loaded = { devices: BtDevice[]; error: string | null } | null;

/** Slow: the profiler behind it takes a second or two per read. */
const POLL_MS = 20_000;
/** Below this a battery is drawn as running out — beside its number, never alone. */
const LOW = 20;

async function load(): Promise<Loaded> {
  const res = await commands.bluetoothDevices();
  return res.status === "ok" ? { devices: res.data, error: null } : { devices: [], error: res.error };
}

/** "Left 100% · Right 98% · Case 63%", or "85%" for one battery. */
function summary(device: BtDevice): string {
  if (!device.connected) return "Not connected";
  if (device.batteries.length === 0) return "Connected";
  return device.batteries
    .map((b) => (b.label ? `${b.label} ${b.percent}%` : `${b.percent}%`))
    .join(" · ");
}

/**
 * Media › Devices: the batteries of the Bluetooth gear on the desk — AirPods
 * left, right and case, headphones, Magic Mouse and Keyboard — and a way to
 * connect the ones that are paired but off.
 */
export function MediaDevices() {
  const [loaded, refresh] = usePolled<Loaded>(load, POLL_MS, null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const connect = (device: BtDevice) => {
    setConnecting(device.address);
    setError(null);
    // The finally also covers a missing Tauri (browser preview), where invoke throws.
    Promise.resolve()
      .then(() => commands.bluetoothConnect(device.address))
      .then((res) => res.status === "error" && setError(`${device.name}: ${res.error}`))
      .catch(() => {})
      .finally(() => {
        setConnecting(null);
        refresh();
      });
  };

  if (!loaded) {
    return <p className="m-0 font-notch text-notch-body text-notch-ink-3">Looking for devices…</p>;
  }
  if (loaded.error) {
    return <Empty title="Couldn't read Bluetooth devices">{loaded.error}</Empty>;
  }
  if (loaded.devices.length === 0) {
    return (
      <Empty title="No Bluetooth devices paired">
        Pair AirPods, headphones or a Magic Mouse in System Settings and their
        batteries show up here.
      </Empty>
    );
  }

  const withBattery = loaded.devices
    .filter((d) => d.connected)
    .flatMap((d) => {
      const percent = lowest(d.batteries);
      return percent === null ? [] : [{ name: d.name, percent }];
    })
    .sort((a, b) => a.percent - b.percent);
  const low = withBattery[0];

  return (
    <>
      {low && (
        <Lead className="mb-3" sub={`${low.name} has the least left`}>
          {low.percent}%
        </Lead>
      )}
      <Label>Paired</Label>
      <List label="Bluetooth devices">
        {loaded.devices.map((device) => {
          const percent = lowest(device.batteries);
          return (
            <Row
              key={device.address || device.name}
              lead={<AppIcon icon={device.kind === "other" ? "bluetooth" : device.kind} label={device.kind} />}
              title={device.name}
              subtitle={summary(device)}
              trailing={
                device.connected ? (
                  percent !== null && (
                    <span className="flex items-center gap-2">
                      <Bar
                        className="w-16"
                        label={`${device.name} battery`}
                        value={percent / 100}
                        tone={percent <= LOW ? "danger" : "ink"}
                      />
                      <Value className="w-9 text-right">{percent}%</Value>
                    </span>
                  )
                ) : (
                  <Button
                    variant="notch-quiet"
                    size="notch"
                    disabled={connecting !== null || !device.address}
                    onClick={() => connect(device)}
                  >
                    {connecting === device.address ? "Connecting…" : "Connect"}
                  </Button>
                )
              }
            />
          );
        })}
      </List>
      {error && <p className="m-0 mt-2 font-notch text-notch-meta text-notch-ink-3">{error}</p>}
    </>
  );
}
