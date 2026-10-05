import type { BtDevice, Device } from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets";
import { lowest } from "../../media";
import { batteryTone } from "../mac-format";

/**
 * What the Sound and Devices boards say, worked out from the data the notch
 * already has. Apart from the components so the words and the ring values,
 * which can silently be wrong, are testable on their own.
 */

/** The Sound board's lead: "Playing through bluetooth", or the Mac itself. */
export function outputLine(output: Pick<Device, "transport"> | undefined): string {
  return `Playing through ${output?.transport ? output.transport.toLowerCase() : "this Mac"}`;
}

/** The microphone switch's word: the state it is in, never the colour alone. */
export function micWord(muted: boolean): string {
  return muted ? "Muted" : "Live";
}

/** The battery a device is judged by: its lowest, and none when it is off or reports none. */
export function deviceLevel(device: BtDevice): number | null {
  return device.connected ? lowest(device.batteries) : null;
}

/** "Left 100% · Right 98% · Case 63%", or "85%" for one battery. */
export function batterySummary(device: BtDevice): string {
  if (!device.connected) return "Not connected";
  if (device.batteries.length === 0) return "Connected";
  return device.batteries
    .map((b) => (b.label ? `${b.label} ${b.percent}%` : `${b.percent}%`))
    .join(" · ");
}

/** Under a row's name: only where the percent beside it cannot say enough. */
export function rowSubtitle(device: BtDevice): string | null {
  if (!device.connected || device.batteries.length !== 1) return batterySummary(device);
  return null;
}

/** Green while it lasts, amber and then red at the same thresholds as Mac Power. */
export function levelAccent(percent: number): WidgetRole {
  const tone = batteryTone(percent, "discharging");
  return tone === "danger" ? "move" : tone === "warn" ? "orange" : "ok";
}

export type Level = { name: string; kind: string; percent: number };

/** The connected devices that report a battery, in the order the profiler lists them. */
export function levels(devices: BtDevice[]): Level[] {
  return devices.flatMap((d) => {
    const percent = deviceLevel(d);
    return percent === null ? [] : [{ name: d.name, kind: d.kind, percent }];
  });
}

/** The one with the least left, which decides when to charge something. First wins a tie. */
export function leastLeft(all: Level[]): Level | null {
  return all.reduce<Level | null>((low, l) => (!low || l.percent < low.percent ? l : low), null);
}

/** What the four rings of the medium tile hold: the first four, as Apple's does. */
export const RING_SLOTS = 4;
