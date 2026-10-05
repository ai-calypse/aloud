import type { Battery, Snap, SystemStats, Thermal } from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets";
import { formatNumeral } from "@/components/notch/widgets";
import { SNAPS } from "../../snaps";
import { batteryWidget } from "../widget-home";
import { chargeSentence, formatGb, num, thermalReading } from "../mac-format";

/**
 * What the Mac widgets say, worked out from the same readings the calm views
 * use. Apart from the components so ring values and words, which can silently
 * be wrong, are testable.
 */

/** 0…1, and 0 for a disk or memory that reports no total (a null from Rust). */
export function usedFraction(used: number | null, total: number | null): number {
  const t = num(total);
  return t > 0 ? Math.min(1, num(used) / t) : 0;
}

export type StatRing = {
  key: "cpu" | "memory" | "disk";
  label: string;
  value: number;
  /** The percentage, as the big number. */
  text: string;
  /** The detail the calm row carries under its name. */
  sub: string;
  accent: WidgetRole;
};

/**
 * CPU, Memory, Disk as Fitness's Move, Exercise and Stand: outermost to
 * innermost, the order a ring stack takes them. Percent of what there is, so
 * the three are comparable at a glance.
 */
export function statRings(s: SystemStats): StatRing[] {
  const cpu = Math.min(1, num(s.cpu) / 100);
  const memory = usedFraction(s.memoryUsedGb, s.memoryTotalGb);
  const disk = usedFraction(s.diskUsedGb, s.diskTotalGb);
  const pct = (v: number) => formatNumeral(Math.round(v * 100));
  return [
    { key: "cpu", label: "CPU", value: cpu, text: pct(cpu), sub: "All cores", accent: "move" },
    {
      key: "memory",
      label: "Memory",
      value: memory,
      text: pct(memory),
      sub: `${formatGb(num(s.memoryUsedGb))} of ${formatGb(num(s.memoryTotalGb))}`,
      accent: "exercise",
    },
    {
      key: "disk",
      label: "Disk",
      value: disk,
      text: pct(disk),
      sub: `${formatGb(Math.max(0, num(s.diskTotalGb) - num(s.diskUsedGb)))} free`,
      accent: "stand",
    },
  ];
}

/**
 * A rate split into the number a numeral draws and the unit beside it, so a
 * 130pt tile never has to fit "812 KB/s" in 32pt Light. Joined with a space
 * it is `formatRate`'s string.
 */
export function rateParts(bytesPerSecond: number | null): { value: string; unit: string } {
  const b = num(bytesPerSecond);
  if (b < 1000) return { value: String(Math.round(b)), unit: "B/s" };
  if (b < 1_000_000) return { value: String(Math.round(b / 1000)), unit: "KB/s" };
  const mb = b / 1_000_000;
  return { value: mb < 100 ? mb.toFixed(1) : String(Math.round(mb)), unit: "MB/s" };
}

/** The heat tile: the word is the status, the accent only echoes it. */
export function thermalWidget(t: Thermal): { word: string; accent: WidgetRole; hot: boolean } {
  const accent: Record<Thermal, WidgetRole> = {
    nominal: "ok",
    fair: "sun",
    serious: "orange",
    critical: "move",
  };
  return { word: thermalReading(t).word, accent: accent[t], hot: t === "serious" || t === "critical" };
}

/** The Batteries widget: the charge ring and the numbers that sit beside it. */
export function powerWidget(b: Battery) {
  const lead = batteryWidget(b);
  return {
    ...lead,
    sentence: chargeSentence(b.state, b.minutes),
    charging: b.state === "charging",
    // A Mac that does not report these gets no tile, not a made-up zero.
    health: b.health != null ? { value: b.health / 100, text: String(b.health) } : null,
    cycles: b.cycles != null ? formatNumeral(b.cycles, { group: true }) : null,
  };
}

/** One privacy tile: whether it is on, in a word, and who holds it. */
export function privacyTile(kind: "mic" | "camera", on: boolean, apps: string[]) {
  if (!on) return { on, word: "Off", detail: kind === "mic" ? "Not listening" : "Not watching" };
  // macOS publishes which app has the mic, never which has the camera.
  const detail =
    kind === "mic" ? apps.join(", ") || "An app is recording" : "An app, which macOS does not name";
  return { on, word: "In use", detail };
}

/**
 * Where each snap lands, drawn as the filled part of a miniature screen.
 * Whole class names so Tailwind sees them; `satisfies` makes a new `Snap`
 * without a drawing a type error rather than a blank glyph.
 */
export const SNAP_REGION = {
  left: "inset-y-0 left-0 w-1/2",
  right: "inset-y-0 right-0 w-1/2",
  top: "inset-x-0 top-0 h-1/2",
  bottom: "inset-x-0 bottom-0 h-1/2",
  topLeft: "top-0 left-0 h-1/2 w-1/2",
  topRight: "top-0 right-0 h-1/2 w-1/2",
  bottomLeft: "bottom-0 left-0 h-1/2 w-1/2",
  bottomRight: "bottom-0 right-0 h-1/2 w-1/2",
  maximize: "inset-0",
  center: "inset-x-1/4 inset-y-1/4",
} satisfies Record<Snap, string>;

/** Halves and quarters share the grid; the whole-window pair sits apart. */
export const GRID_SNAPS = SNAPS.filter((s) => s.group !== "Whole");
export const WHOLE_SNAPS = SNAPS.filter((s) => s.group === "Whole");
