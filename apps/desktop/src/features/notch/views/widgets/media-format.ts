import type { BtDevice, Device } from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets";
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

/** The headline charge, which Rust works out (buds' mean, never the case); none while off. */
export function deviceLevel(device: BtDevice): number | null {
  return device.connected ? device.level : null;
}

/**
 * Under a row's name, the small print the headline cannot carry: that a lone
 * bud is the figure, and the case, which is never the headline. Nothing when
 * the percent beside it says it all.
 */
export function rowSubtitle(device: BtDevice): string | null {
  if (!device.connected) return "Not connected";
  const parts = [
    device.singleBud && `${device.singleBud} bud only`,
    device.case !== null && `Case ${device.case}%`,
  ].filter(Boolean);
  if (parts.length) return parts.join(" · ");
  return device.level === null ? "Connected" : null;
}

/** Green while it lasts, amber and then red at the same thresholds as Mac Power. */
export function levelAccent(percent: number): WidgetRole {
  const tone = batteryTone(percent, "discharging");
  return tone === "danger" ? "move" : tone === "warn" ? "orange" : "ok";
}
