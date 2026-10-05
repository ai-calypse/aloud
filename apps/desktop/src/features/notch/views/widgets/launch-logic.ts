import type { WidgetRole } from "@/components/notch/widgets";
import type { Run } from "../Launch";

/**
 * Colours a shortcut tile may wear. Shortcuts' own palette also has orange,
 * yellow, green and teal, but white words on those fills (the accent at 76%
 * over black) fall under AA, so the set is the hues that clear it.
 */
export const SHORTCUT_ROLES: WidgetRole[] = ["media", "blue", "podcast", "agent", "calendar"];

/**
 * A shortcut's colour, from its name: the same shortcut keeps its colour when
 * a filter or a new shortcut moves it, which an index would not.
 */
export function shortcutRole(name: string): WidgetRole {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.codePointAt(0)!) >>> 0;
  return SHORTCUT_ROLES[h % SHORTCUT_ROLES.length];
}

/** Consecutive groups of `size`; the last may be short. */
export function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}

/** What a tile says about a shortcut's last run, in words. */
export function runWord(run: Run | undefined): string | null {
  return run?.state === "running" ? "Running…" : run?.state === "done" ? "Done" : run?.state === "failed" ? "Failed" : null;
}
