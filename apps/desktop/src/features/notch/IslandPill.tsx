import type { AgentPermission, AgentSession, Island } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import { LevelBar } from "@/components/notch/level-bar";
import { AgentCats } from "@/features/overlay/NotchAgents";
import { capsuleText, islandRole, islandTone, levelOf, pillSides } from "./islands";

/**
 * An island's lead, wherever it is drawn: the coding agents' cat stack for
 * the `agents` island — the face they had before there were islands — and
 * the island's own icon for everything else.
 */
export function IslandIcon({
  island,
  sessions,
  permissions,
  size = "sm",
}: {
  island: Island;
  sessions: AgentSession[];
  permissions: AgentPermission[];
  size?: "sm" | "md";
}) {
  if (island.id === "agents" && sessions.length > 0) {
    const cats = <AgentCats sessions={sessions} permissions={permissions} />;
    // In a row it holds the icon column's width, so titles line up.
    return size === "md" ? <span className="flex min-w-[26px] justify-center">{cats}</span> : cats;
  }
  return <AppIcon icon={island.icon} label={island.title} size={size} />;
}

/**
 * The closed notch with live activities: ranked, either side of the camera,
 * never under it — the middle column is exactly the cutout, which has no
 * pixels to draw on. The notch's own spring widens it to fit (useNotchPanel
 * measures this), so a second activity arriving is the pill growing, not a
 * swap.
 *
 * A whisper (`whisper`) borrows it for a moment instead: the same widened
 * notch, one glyph left of the camera and a word, a number or a level bar right,
 * and nothing else — the live activities stand aside until it leaves.
 */
export function IslandPill({
  live,
  sessions,
  permissions,
  leaving,
  whisper,
  waiting = 0,
}: {
  live: Island[];
  sessions: AgentSession[];
  permissions: AgentPermission[];
  /** The last one went: content fades ahead of the collapse clipping it. */
  leaving: boolean;
  /** A takeover whisper on screen, which replaces the live activities. */
  whisper?: Island | null;
  /** Takeovers waiting their turn: "+N" beside whatever is showing. */
  waiting?: number;
}) {
  const { left, right, more } = pillSides(live);
  const single = live.length === 1 ? live[0] : null;
  const icon = (island: Island) => (
    <IslandIcon island={island} sessions={sessions} permissions={permissions} />
  );

  if (whisper) return <Whisper island={whisper} waiting={waiting} leaving={leaving} />;
  const rest = more + waiting;

  return (
    <div
      className="island-pill"
      data-leaving={leaving || undefined}
      role="status"
      aria-label={live.map((i) => [i.title, i.detail].filter(Boolean).join(": ")).join(", ")}
    >
      <div className="island-pill__side island-pill__side--left">
        {left.map((island) => (
          <span className="island-pill__item" key={island.id} data-w-role={islandRole(island)}>
            {icon(island)}
            {/* One activity spans both sides: its value goes right. */}
            {!single && <Detail island={island} />}
          </span>
        ))}
      </div>
      <span className="island-pill__camera" aria-hidden />
      <div
        className="island-pill__side island-pill__side--right"
        // One activity's value sits here, outside its item.
        data-w-role={single ? islandRole(single) : undefined}
      >
        {single ? (
          <Detail island={single} />
        ) : (
          right.map((island) => (
            <span className="island-pill__item" key={island.id} data-w-role={islandRole(island)}>
              {icon(island)}
              <Detail island={island} />
            </span>
          ))
        )}
        {rest > 0 && <span className="island-pill__more">+{rest}</span>}
      </div>
    </div>
  );
}

/**
 * An island's live value. Urgent carries a dot, but the word says it too —
 * status is never colour alone (PRODUCT.md).
 */
function Detail({ island }: { island: Island }) {
  if (!island.detail) return null;
  return (
    <span className="island-pill__detail" data-urgent={island.priority === "urgent" || undefined}>
      {island.detail}
    </span>
  );
}

/**
 * A whisper: the notch's own height, widened either side of the camera. It
 * speaks once, in a glyph and one thing — a word, a number, or a level bar with
 * its number — and never has a button (one is a hover away, in the peek it
 * opens). A newer whisper of the same kind arrives as an update to this one, so
 * the bar moves rather than the whole thing remounting.
 */
export function Whisper({
  island,
  waiting = 0,
  leaving = false,
}: {
  island: Island;
  waiting?: number;
  leaving?: boolean;
}) {
  const level = levelOf(island);
  // A bar carries the number beside it; a bare level is its own word.
  const text = capsuleText(island);
  return (
    <div
      className="island-pill island-pill--whisper"
      data-leaving={leaving || undefined}
      data-tone={islandTone(island)}
      data-w-role={islandRole(island)}
      role="status"
      aria-label={[island.title, island.detail].filter(Boolean).join(": ")}
    >
      <div className="island-pill__side island-pill__side--left">
        <AppIcon icon={island.icon} label={island.title} size="sm" />
      </div>
      <span className="island-pill__camera" aria-hidden />
      <div className="island-pill__side island-pill__side--right">
        {level !== null && <LevelBar level={level} label={island.title} />}
        {text && <span className="island-pill__detail island-pill__value">{text}</span>}
        {waiting > 0 && <span className="island-pill__more">+{waiting}</span>}
      </div>
    </div>
  );
}
