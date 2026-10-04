import type { AgentPermission, AgentSession, Island } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import { AgentCats } from "@/features/overlay/NotchAgents";
import { pillSides } from "./islands";

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
 */
export function IslandPill({
  live,
  sessions,
  permissions,
  leaving,
}: {
  live: Island[];
  sessions: AgentSession[];
  permissions: AgentPermission[];
  /** The last one went: content fades ahead of the collapse clipping it. */
  leaving: boolean;
}) {
  const { left, right, more } = pillSides(live);
  const single = live.length === 1 ? live[0] : null;
  const icon = (island: Island) => (
    <IslandIcon island={island} sessions={sessions} permissions={permissions} />
  );

  return (
    <div
      className="island-pill"
      data-leaving={leaving || undefined}
      role="status"
      aria-label={live.map((i) => [i.title, i.detail].filter(Boolean).join(": ")).join(", ")}
    >
      <div className="island-pill__side island-pill__side--left">
        {left.map((island) => (
          <span className="island-pill__item" key={island.id}>
            {icon(island)}
            {/* One activity spans both sides: its value goes right. */}
            {!single && <Detail island={island} />}
          </span>
        ))}
      </div>
      <span className="island-pill__camera" aria-hidden />
      <div className="island-pill__side island-pill__side--right">
        {single ? (
          <Detail island={single} />
        ) : (
          right.map((island) => (
            <span className="island-pill__item" key={island.id}>
              {icon(island)}
              <Detail island={island} />
            </span>
          ))
        )}
        {more > 0 && <span className="island-pill__more">+{more}</span>}
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
