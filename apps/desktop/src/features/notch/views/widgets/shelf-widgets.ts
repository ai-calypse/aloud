import type { WidgetRole } from "@/components/notch/widgets";
import type { ShelfDrive, ShelfItem } from "@/lib/bindings";
import { formatBytes } from "../shelf-data";

/**
 * What the Shelf's widget tiles say, worked out from the same rows the calm
 * views read. Apart from the components so the words, the thresholds and
 * which actions a file earns (which can silently be wrong) are testable.
 */

/** Pictures the notch can preview: the calm Downloads view's own test, kept in step. */
const IMAGE = /\.(png|jpe?g|gif|heic|webp|tiff?)$/i;
export const isImageName = (name: string): boolean => IMAGE.test(name);

/** A zip on the shelf can be unpacked; a folder named `x.zip` cannot. */
export function canUnzip(item: Pick<ShelfItem, "kind" | "name">): boolean {
  return item.kind === "file" && /\.zip$/i.test(item.name);
}

/** The line under a shelved file's name. */
export function itemMeta(item: Pick<ShelfItem, "kind" | "size">): string {
  return item.kind === "folder" ? "Folder" : formatBytes(item.size);
}

/** The chosen row, or nothing: a selection whose file has gone is no selection. */
export function selectedItem<T extends { id: string }>(items: T[], id: string | null): T | null {
  return (id && items.find((i) => i.id === id)) || null;
}

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

/** The newest screenshot is the hero; the rest are a grid behind it. */
export function splitShots<T>(shots: T[]): { latest: T | null; rest: T[] } {
  return { latest: shots[0] ?? null, rest: shots.slice(1) };
}
