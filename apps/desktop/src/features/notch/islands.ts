import type { Island, IslandChanged, IslandTier } from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets/roles";

/**
 * The webview's copy of the island queue after one `island` event: the change
 * applied, then ordered by the ranking Rust sent with it. There is no second
 * ranking here to disagree with the crate's; an id missing from `order` (a
 * change that raced its own event) goes last rather than vanishing.
 */
export function applyIsland(islands: Island[], { change, order }: IslandChanged): Island[] {
  let next: Island[];
  switch (change.op) {
    case "add":
    case "update":
      next = [...islands.filter((i) => i.id !== change.value.id), change.value];
      break;
    case "remove":
      next = islands.filter((i) => i.id !== change.value);
      break;
  }
  const rank = new Map(order.map((id, index) => [id, index]));
  const at = (island: Island) => rank.get(island.id) ?? order.length;
  return next.sort((a, b) => at(a) - at(b));
}

/**
 * How many activities the closed notch draws beside the camera before the
 * rest become a count: two a side, the most a glance takes in.
 */
export const PILL_MAX = 4;

/**
 * The closed notch's two sides. One activity spans both — its icon left of
 * the camera, its value right — as the Dynamic Island draws one; more
 * alternate by rank, so the first two are always nearest the eye's start,
 * and past `PILL_MAX` the remainder is a count on the right.
 */
export function pillSides(live: Island[]): {
  left: Island[];
  right: Island[];
  more: number;
} {
  if (live.length <= 1) return { left: live, right: [], more: 0 };
  const shown = live.slice(0, PILL_MAX);
  return {
    left: shown.filter((_, i) => i % 2 === 0),
    right: shown.filter((_, i) => i % 2 === 1),
    more: live.length - shown.length,
  };
}

/** The accent each owner's island speaks in, by the prefix of its id (`media:browser:…`). */
const ISLAND_ROLES: Record<string, WidgetRole> = {
  agents: "agent",
  media: "media",
  timer: "move",
  battery: "ok",
  download: "info",
  calendar: "calendar",
  // The takeovers from Shelf and the clipboard; a run's own `toast:` keeps the
  // shared ink and speaks green through `islandTone` instead.
  shot: "info",
  drive: "info",
  clipboard: "info",
};

/**
 * The widgets look's colour for an island: its owner's accent. None for an
 * urgent island (its own warning treatment and its word stand) and for an owner
 * with no role, which keep the shared ink. Colour only; the island's words and
 * geometry are the same in both looks.
 */
export function islandRole(island: Island): WidgetRole | undefined {
  if (island.priority === "urgent") return undefined;
  return ISLAND_ROLES[island.id.split(":")[0]];
}

/**
 * The size an island takes over at, as Rust's `Island::takeover_tier`: its own
 * tier, else a peek for an alert (every push made before tiers), else none —
 * a live island is state, not a takeover.
 */
export function takeoverTier(island: Island): IslandTier | null {
  return island.tier ?? (island.kind === "alert" ? "peek" : null);
}

/**
 * Drawn as a side capsule, never a row or a card: a whisper, or a peek or card
 * the gate shrank because the user is typing (`shrunk`).
 */
export function isCapsule(island: Island): boolean {
  return takeoverTier(island) === "whisper" || island.shrunk === true;
}

/**
 * Whether pointing at a capsule opens the next size up: it has something to do
 * (a button) or was shrunk from something bigger. A whisper with neither says
 * all it has.
 */
export function opensOnHover(island: Island): boolean {
  return island.shrunk === true || island.actions.length > 0;
}

/**
 * How long it stays, in milliseconds, before leaving by itself; null for a
 * card, which stays until answered. Rust resolves this (`gate::dwell_ms`) and
 * sends it; the fallback is the same table for a push that skipped the gate.
 */
export function dwellMs(island: Island): number | null {
  const tier = takeoverTier(island);
  if (tier === "card") return null;
  if (island.dwellMs != null) return island.dwellMs;
  return tier === "whisper" ? 1800 : 4000;
}

/** A level, kept inside the bar. */
export function levelOf(island: Island): number | null {
  return island.level == null ? null : Math.min(100, Math.max(0, island.level));
}

export type Tone = "ok" | "warn" | "danger";

/**
 * The colour a takeover's glyph speaks in, where the word alone is not
 * enough: red for what is critical, green for a write that landed. Keyed on
 * the id's prefix, the way `ISLAND_ROLES` is; none leaves the shared ink.
 */
const ISLAND_TONES: [prefix: string, tone: Tone][] = [
  ["mac:battery", "danger"],
  ["mac:privacy", "warn"],
  ["toast:", "ok"],
  ["clipboard:", "ok"],
];

export function islandTone(island: Island): Tone | undefined {
  if (island.priority === "urgent") return "danger";
  return ISLAND_TONES.find(([prefix]) => island.id.startsWith(prefix))?.[1];
}

/**
 * What a capsule says: its one word, else its level, else the title cut to a
 * word's worth. A whisper is a glyph and at most one of these.
 */
export function capsuleText(island: Island): string | null {
  if (island.short) return island.short;
  const level = levelOf(island);
  if (level !== null) return `${Math.round(level)}`;
  return island.title ? island.title : null;
}
