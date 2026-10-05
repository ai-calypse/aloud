import type { WidgetRole } from "@/components/notch/widgets";
import type { ShelfDrive } from "@/lib/bindings";
import { formatBytes } from "../shelf-data";

/**
 * What the Shelf's Downloads and Drives tiles say, worked out from the rows
 * the shelf lists. Apart from the components so the words and the thresholds,
 * which can silently be wrong, are testable.
 */

/** Pictures the notch can preview: what the Downloads tiles preview. */
const IMAGE = /\.(png|jpe?g|gif|heic|webp|tiff?)$/i;
export const isImageName = (name: string): boolean => IMAGE.test(name);

/**
 * A drive's ring: how full it is, in words as well as in the arc, with the
 * accent it earns. Green while there is room, amber as it fills, red when
 * the next copy may not fit. A drive that reports no size has no ring to draw.
 */
export function driveUsage(drive: Pick<ShelfDrive, "size" | "free">): {
  value: number;
  percent: number | null;
  word: string;
  accent: WidgetRole;
} {
  if (drive.size <= 0) return { value: 0, percent: null, word: "", accent: "ok" };
  const value = Math.min(1, Math.max(0, (drive.size - drive.free) / drive.size));
  const percent = Math.round(value * 100);
  if (value >= 0.95) return { value, percent, word: "Almost full", accent: "move" };
  if (value >= 0.8) return { value, percent, word: "Getting full", accent: "orange" };
  return { value, percent, word: "", accent: "ok" };
}

/** `4.1 GB free of 16 GB`: the one line a drive's tile leads its details with. */
export function driveRoom(drive: Pick<ShelfDrive, "size" | "free">): string {
  return `${formatBytes(drive.free)} free of ${formatBytes(drive.size)}`;
}
