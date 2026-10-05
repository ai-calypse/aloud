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
 * The closed notch's measures, in px. CSS reads them off the pill (IslandPill
 * sets them as custom properties), so the helper below and the drawing cannot
 * disagree about how wide a stack is.
 */
export const PILL = {
  icon: 18,
  gap: 6,
  /** `.island-pill__detail`'s 12ch cap, at the pill's 12px digits. */
  detail: 84,
  disc: 22,
  /** How much of a disc the next one covers, as an avatar stack does. */
  overlap: 7,
  /** The "+N" after the stack. */
  more: 24,
} as const;

/** Discs in the stack before the rest become a count. */
export const STACK_MAX = 3;

/**
 * What the closed notch draws for its live activities, whatever their number:
 * the most important in full (`lead`), the next few as overlapping glyph discs
 * (`stack`, nearest the camera first), and the remainder as a count. It never
 * grows with the number of activities, which is what keeps the notch the size
 * of a notch.
 */
export function pillLayout(live: Island[]): { lead: Island | null; stack: Island[]; more: number } {
  const [lead = null, ...rest] = live;
  return { lead, stack: rest.slice(0, STACK_MAX), more: Math.max(0, rest.length - STACK_MAX) };
}

/**
 * Each side's width in px. One activity spans the camera (icon left, value
 * right); more put the lead's icon and value left and the stack right.
 * Pill-wide, the notch is twice the wider side plus the camera.
 */
export function pillSideWidths(live: Island[]): { left: number; right: number } {
  const { lead, stack, more } = pillLayout(live);
  if (!lead) return { left: 0, right: 0 };
  if (stack.length === 0) return { left: PILL.icon, right: PILL.detail };
  const discs = PILL.disc + (stack.length - 1) * (PILL.disc - PILL.overlap);
  return {
    left: PILL.icon + PILL.gap + PILL.detail,
    right: discs + (more > 0 ? PILL.gap + PILL.more : 0),
  };
}

/** The widest a side ever gets: the lead in full, or a full stack and its count. */
export const PILL_SIDE_MAX = Math.max(
  PILL.icon + PILL.gap + PILL.detail,
  PILL.disc + (STACK_MAX - 1) * (PILL.disc - PILL.overlap) + PILL.gap + PILL.more,
);

/** The accent each owner's island speaks in, by the prefix of its id (`media:browser:…`). */
const ISLAND_ROLES: Record<string, WidgetRole> = {
  agents: "agent",
  media: "media",
  timer: "move",
  battery: "ok",
  calendar: "calendar",
  // The takeovers from Shelf and the clipboard; a run's own `toast:` keeps the
  // shared ink and speaks green through `islandTone` instead.
  shot: "info",
  clipboard: "info",
  // The system events, as docs/design/notch-events.html colours them: drives,
  // downloads and the network cyan, devices and displays info blue.
  drive: "cyan",
  download: "cyan",
  net: "cyan",
  bt: "info",
  display: "info",
};

/**
 * The events whose colour is their state, so the prefix alone cannot say it.
 * Each reads the glyph or word Rust already chose for that state.
 */
function stateRole({ id, icon, short }: Island): WidgetRole | undefined {
  if (id === "hud:volume") return "orange";
  if (id === "hud:brightness") return "sun";
  if (id === "hud:caps") return "info";
  if (id === "hud:mic") return icon === "mic-off" ? "alert" : "ok";
  if (id.startsWith("bt:low:")) return "sun";
  if (id === "power:charger") return icon === "bolt" ? "ok" : undefined;
  if (id === "power:lowpower") return short === "Low Power" ? "sun" : undefined;
  // `mac:battery-full` shares the low battery's prefix.
  if (id === "mac:battery-full") return "ok";
  if (id.startsWith("mac:battery")) return "alert";
  // Apple's own dots: green camera, orange microphone.
  if (id.startsWith("mac:privacy")) return icon === "camera" ? "ok" : "orange";
  return undefined;
}

/**
 * The colour for an island: its owner's accent. None for an
 * urgent island (its own warning treatment and its word stand) and for an owner
 * with no role, which keep the shared ink. Colour only; the island's words and
 * geometry do not depend on it.
 */
export function islandRole(island: Island): WidgetRole | undefined {
  if (island.priority === "urgent") return undefined;
  return stateRole(island) ?? ISLAND_ROLES[island.id.split(":")[0]];
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
 * The colour a takeover's glyph speaks in, where the word alone
 * is not enough: red for what is critical, green for a write that landed.
 * Everything else takes its owner's accent (`islandRole`), so a 20% battery
 * is red there. Keyed on the id's prefix; none leaves the shared ink.
 */
const ISLAND_TONES: [prefix: string, tone: Tone][] = [
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
