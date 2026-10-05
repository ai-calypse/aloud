import { useState } from "react";
import { BluetoothIcon } from "lucide-react";
import { commands, type BtDevice } from "@/lib/bindings";
import { Capsule, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { cn } from "@/lib/utils";
import { usePolled } from "../../media";
import { deviceLevel, levelAccent, rowSubtitle } from "./media-format";
import { kindIcon, Notice, Pill } from "./parts";

type Loaded = { devices: BtDevice[]; error: string | null } | null;

/** Slow: the profiler behind it takes a second or two per read. */
const POLL_MS = 20_000;

async function load(): Promise<Loaded> {
  const res = await commands.bluetoothDevices();
  return res.status === "ok" ? { devices: res.data, error: null } : { devices: [], error: res.error };
}

/**
 * Media › Devices as a widget board: Apple's large Batteries list, a row per
 * paired device with its charge (and a Connect button for the ones that are
 * off). The medium ring tile and small "least left" tile it used to sit with
 * drew the same devices again, so they are gone. Read on a slow clock.
 */
export function WidgetMediaDevices() {
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
  return <DevicesBoard loaded={loaded} connecting={connecting} error={error} onConnect={connect} />;
}

export function DevicesBoard({
  loaded,
  connecting,
  error,
  onConnect,
}: {
  loaded: Loaded;
  connecting: string | null;
  error: string | null;
  onConnect: (device: BtDevice) => void;
}) {
  if (!loaded || loaded.error || loaded.devices.length === 0) {
    return (
      <WidgetBoard label="Devices">
        <WidgetFrame size="medium" label="Batteries" ground="vibrant">
          <Notice
            icon={<BluetoothIcon strokeWidth={1.6} />}
            title={
              !loaded
                ? "Looking for devices…"
                : loaded.error
                  ? "Couldn't read Bluetooth devices"
                  : "No Bluetooth devices paired"
            }
          >
            {loaded?.error ? (
              <p className="m-0">{loaded.error}</p>
            ) : (
              loaded && (
                <p className="m-0">
                  Pair AirPods, headphones or a Magic Mouse in System Settings and their batteries show up here.
                </p>
              )
            )}
          </Notice>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  return (
    // One large tile: a two-column board, so it sits centred, not left of a hole.
    <WidgetBoard label="Devices" className="notch-board--solo">
      <ListTile devices={loaded.devices} connecting={connecting} error={error} onConnect={onConnect} />
    </WidgetBoard>
  );
}

/**
 * Beside the name: the headline charge (the buds' mean for AirPods), drawn as
 * a capsule in green until it runs low. The case is small print under the name.
 */
function ListTile({
  devices,
  connecting,
  error,
  onConnect,
}: {
  devices: BtDevice[];
  connecting: string | null;
  error: string | null;
  onConnect: (device: BtDevice) => void;
}) {
  return (
    <WidgetFrame size="large" label="Batteries" ground="vibrant">
      <div className="flex h-full flex-col px-3.5">
        <ul aria-label="Bluetooth devices" className="m-0 flex min-h-0 flex-1 list-none flex-col overflow-y-auto p-0">
          {devices.map((device) => {
            const percent = deviceLevel(device);
            const Glyph = kindIcon(device.kind);
            const subtitle = rowSubtitle(device);
            return (
              <li
                key={device.address || device.name}
                className="flex min-h-14 shrink-0 items-center gap-3 border-b border-widget-sep last:border-b-0"
              >
                <Glyph
                  aria-hidden
                  className={cn("size-5 shrink-0", device.connected ? "text-widget-ink" : "text-widget-ink-2")}
                  strokeWidth={1.6}
                />
                <span className="min-w-0 flex-1">
                  <span
                    className={cn(
                      "block truncate text-notch-w-body leading-tight",
                      device.connected ? "font-medium" : "font-medium text-widget-ink-2",
                    )}
                  >
                    {device.name}
                  </span>
                  {subtitle && (
                    <span className="block truncate text-notch-w-meta leading-tight text-widget-ink-2">
                      {subtitle}
                    </span>
                  )}
                </span>
                {!device.connected ? (
                  <Pill
                    disabled={connecting !== null || !device.address}
                    aria-label={`Connect ${device.name}`}
                    onClick={() => onConnect(device)}
                  >
                    {connecting === device.address ? "Connecting…" : "Connect"}
                  </Pill>
                ) : (
                  percent !== null && (
                    <span className="flex shrink-0 items-center gap-2.5">
                      <span className="w-9 text-right text-notch-w-meta font-medium tabular-nums">{percent}%</span>
                      <Capsule
                        className="h-2.5 w-14"
                        label={`${device.name} battery`}
                        value={percent / 100}
                        accent={levelAccent(percent)}
                      />
                    </span>
                  )
                )}
              </li>
            );
          })}
        </ul>
        {error && <p className="m-0 pb-3 text-notch-w-meta leading-snug text-widget-ink-2">{error}</p>}
      </div>
    </WidgetFrame>
  );
}
