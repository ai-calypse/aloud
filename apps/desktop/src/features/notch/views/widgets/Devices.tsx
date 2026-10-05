import { useState } from "react";
import { BluetoothIcon } from "lucide-react";
import { commands, type BtDevice } from "@/lib/bindings";
import { Capsule, formatNumeral, Numeral, Ring, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { cn } from "@/lib/utils";
import { usePolled } from "../../media";
import {
  deviceLevel,
  leastLeft,
  levelAccent,
  levels,
  RING_SLOTS,
  rowSubtitle,
  type Level,
} from "./media-format";
import { kindIcon, Notice, Pill } from "./parts";

type Loaded = { devices: BtDevice[]; error: string | null } | null;

/** Slow: the profiler behind it takes a second or two per read. */
const POLL_MS = 20_000;

async function load(): Promise<Loaded> {
  const res = await commands.bluetoothDevices();
  return res.status === "ok" ? { devices: res.data, error: null } : { devices: [], error: res.error };
}

/**
 * Media › Devices in the widgets look: Apple's Batteries family. The large
 * list is every paired device with its charge (and a Connect button for the
 * ones that are off), the medium tile is four rings, the small one the device
 * with the least left. The same read as the calm view, on the same slow clock.
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
  const all = levels(loaded.devices);
  const low = leastLeft(all);
  return (
    <WidgetBoard label="Devices">
      <ListTile devices={loaded.devices} connecting={connecting} error={error} onConnect={onConnect} />
      <RingsTile levels={all} />
      <LeastTile low={low} />
    </WidgetBoard>
  );
}

/** Beside the name: the charge, and the battery drawn as a capsule in green until it runs low. */
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

/** The medium tile: a ring a device, its glyph inside, the charge and the name under it. */
function RingsTile({ levels: all }: { levels: Level[] }) {
  const shown = all.slice(0, RING_SLOTS);
  return (
    <WidgetFrame size="medium" label="Batteries" ground="vibrant">
      {shown.length === 0 ? (
        <Notice icon={<BluetoothIcon strokeWidth={1.6} />} title="No batteries to show">
          <p className="m-0">Connected devices that report a charge appear here.</p>
        </Notice>
      ) : (
        <ul className="m-0 grid h-full list-none grid-cols-4 items-center p-0 px-3.5">
          {shown.map((l) => {
            const Glyph = kindIcon(l.kind);
            return (
              <li key={l.name} className="flex min-w-0 flex-col items-center gap-1">
                <Ring
                  value={l.percent / 100}
                  label={`${l.name} battery`}
                  valueText={`${l.percent}%`}
                  accent={levelAccent(l.percent)}
                  size={60}
                  stroke={7}
                >
                  <Glyph aria-hidden className="size-5 text-widget-ink" strokeWidth={1.8} />
                </Ring>
                <span className="text-notch-w-headline leading-tight font-light tabular-nums">{l.percent}%</span>
                <span className="w-full truncate px-0.5 text-center text-notch-w-over leading-tight font-medium text-widget-ink-2">
                  {l.name}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </WidgetFrame>
  );
}

/** The small tile: the device that will need charging first, as Apple's shows its one ring. */
function LeastTile({ low }: { low: Level | null }) {
  if (!low) {
    return (
      <WidgetFrame size="small" label="Batteries" ground="vibrant">
        <Notice icon={<BluetoothIcon strokeWidth={1.6} />} title="No battery reading" />
      </WidgetFrame>
    );
  }
  const Glyph = kindIcon(low.kind);
  return (
    <WidgetFrame size="small" label="Batteries" ground="vibrant" accent={levelAccent(low.percent)}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <div className="flex items-start justify-between gap-2">
          <Ring
            value={low.percent / 100}
            label={`${low.name} battery`}
            valueText={`${low.percent}%`}
            size={56}
            stroke={9}
          >
            <Glyph aria-hidden className="size-5 text-widget-ink" strokeWidth={1.8} />
          </Ring>
          <span className="line-clamp-2 pt-1 text-right text-notch-w-meta leading-tight font-medium text-widget-ink-2">
            {low.name} has the least left
          </span>
        </div>
        <div className="flex items-baseline">
          <Numeral text={formatNumeral(low.percent)} size="l" roll />
          <span className="text-notch-w-numeral-s font-light text-widget-ink">%</span>
        </div>
      </div>
    </WidgetFrame>
  );
}
