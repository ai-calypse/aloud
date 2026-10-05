import type { Island, IslandChanged } from "@/lib/bindings";
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
