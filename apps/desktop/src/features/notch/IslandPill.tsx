import { useRef, type CSSProperties } from "react";
import type { AgentPermission, AgentSession, Island } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import { LevelBar } from "@/components/notch/level-bar";
import { AgentCats } from "@/features/overlay/NotchAgents";
import { capsuleText, islandRole, islandTone, levelOf, PILL, pillLayout } from "./islands";
import { useFlip } from "./useFlip";

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
 * The closed notch with live activities, either side of the camera, never
 * under it — the middle column is exactly the cutout, which has no pixels to
 * draw on. The most important activity is drawn in full; the rest are glyph
 * discs stacked over one another beside the camera, with a count past three
 * (`pillLayout`), so the notch is never wider for having more going on. The
 * notch's own spring widens it to fit (useNotchPanel measures this) when the
 * first of them arrives.
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
  const { lead, stack, more } = pillLayout(live);
  const stackRef = useRef<HTMLDivElement>(null);
  // The discs slide into their new places when one joins, leaves or re-ranks.
  useFlip(stackRef, stack.map((i) => i.id).join("|"));

  if (whisper) return <Whisper island={whisper} waiting={waiting} leaving={leaving} />;
  const rest = more + waiting;

  return (
    <div
      className="island-pill"
      data-leaving={leaving || undefined}
      role="status"
      aria-label={live.map((i) => [i.title, i.detail].filter(Boolean).join(": ")).join(", ")}
      style={
        {
          "--disc": `${PILL.disc}px`,
          "--disc-overlap": `${PILL.overlap}px`,
        } as CSSProperties
      }
    >
      <div className="island-pill__side island-pill__side--left">
        {lead && (
          <span className="island-pill__item" data-w-role={islandRole(lead)}>
            <IslandIcon island={lead} sessions={sessions} permissions={permissions} />
          </span>
        )}
        {stack.length > 0 && (
          // The others tuck in after the lead's icon, each under the one
          // before it, so the stack reads outward from the lead.
          <div className="island-pill__stack" ref={stackRef}>
            {stack.map((island, i) => (
              <span
                className="island-pill__disc"
                key={island.id}
                data-tile={island.id}
                data-w-role={islandRole(island)}
                style={{ zIndex: stack.length - i }}
                title={island.title}
              >
                <AppIcon icon={island.icon} label={island.title} size="sm" />
              </span>
            ))}
          </div>
        )}
      </div>
      <span className="island-pill__camera" aria-hidden />
      <div
        className="island-pill__side island-pill__side--right"
        // The lead's value sits here, outside its item.
        data-w-role={lead ? islandRole(lead) : undefined}
      >
        {lead && <Detail island={lead} />}
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
