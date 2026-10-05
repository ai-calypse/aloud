import { useEffect, useState } from "react";
import type { AgentPermission, AgentSession } from "@/lib/bindings";
import {
  agentStage,
  agentHue,
  stageCat,
  stageSeed,
  STAGE_LABEL,
  type AgentStage,
} from "@/lib/agent-stage";
import { AgentAvatar } from "./agentIdentity";

/**
 * The "I am listening" cat. The meme captions itself, so it only has to appear
 * while the app is genuinely listening for the joke to land — the caller gates
 * that (dictating, or an agent you spoke). A fixed asset, not a stage cat:
 * the gag is this one specific caption, not "a cat for the current state".
 */
export function ListeningCat() {
  return (
    <img
      className="drop-pill__cat"
      src="/cats/waiting_input/2.png"
      alt="A smug cat, paws crossed, captioned “I am listening”"
    />
  );
}

/**
 * The blown-up meme on avatar hover. Anchored to the avatar's on-screen box and
 * grown from there — the small round cat loses the joke, this gives it back.
 * CSS owns the grow (the target never changes mid-flight), so it is a plain
 * mount + a class the next frame flips.
 */
export function CatPeek({ stage, seed, rect }: { stage: AgentStage; seed: string; rect: DOMRect }) {
  const [shown, setShown] = useState(false);
  // A frame after mount, so the transition has a from-state to leave.
  useEffect(() => {
    const id = requestAnimationFrame(() => setShown(true));
    return () => cancelAnimationFrame(id);
  }, []);
  const SIZE = 180;
  // Grow up-and-right out of the avatar, clamped to the viewport so it never
  // clips off the top or right edge of the overlay canvas.
  const left = Math.min(rect.left, window.innerWidth - SIZE - 8);
  const top = Math.max(8, rect.bottom - SIZE);
  return (
    <div
      className="cat-peek"
      data-shown={shown || undefined}
      aria-hidden
      style={{
        left: `${left}px`,
        top: `${top}px`,
        width: `${SIZE}px`,
        // Origin = where the avatar sits relative to the peek's top-left, so it
        // scales out of the cat rather than out of its own centre.
        transformOrigin: `${rect.left - left + rect.width / 2}px ${rect.top - top + rect.height / 2}px`,
      }}
    >
      <img className="cat-peek__img" src={stageCat(stage, seed)} alt="" />
      <span className="cat-peek__cap">{STAGE_LABEL[stage]}</span>
    </div>
  );
}

/** How many stage cats the closed notch shows before collapsing to a `+N`. */
const AMBIENT_CATS = 4;

/**
 * The coding agents' face in the closed notch: a tight stack of stage cats,
 * drawn as the lead of the `agents` island left of the camera (see
 * `IslandPill`). Hovering opens Home, not these.
 *
 * As an island it is the pill's content, so the pill's measured box is the
 * click-box and there is one writer of it, not two.
 */
export function AgentCats({
  sessions,
  permissions,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
}) {
  const asking = permissions.length > 0;
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  // Busy-first from Rust; the busy ones are what the sign is about. Asking but
  // nothing busy (a parked agent) still needs a face, so fall back to it.
  const busy = sessions.filter((s) => s.busy);
  const cats = ambientCats(sessions);
  const extra = (busy.length > 0 ? busy.length : sessions.length) - cats.length;

  return (
    <span
      className="ambient"
      data-asking={asking || undefined}
      data-busy={busy.length > 0 || undefined}
    >
      <span className="ambient__cats">
        {cats.map((session) => {
          const stage = agentStage(session, askingIds.has(session.id));
          return (
            <AgentAvatar
              key={session.id}
              className="ambient__cat-img"
              stage={stage}
              seed={stageSeed(session.id, stage)}
              hue={agentHue(session.id)}
              data-asking={askingIds.has(session.id) || undefined}
            />
          );
        })}
        {extra > 0 && <span className="ambient__count">+{extra}</span>}
      </span>
    </span>
  );
}

/**
 * The cats the closed notch shows: the busy agents, or — if none are busy but
 * one is asking — that one, so the sign has a face to ring. Capped at
 * `AMBIENT_CATS`.
 */
function ambientCats(sessions: AgentSession[]): AgentSession[] {
  const busy = sessions.filter((s) => s.busy);
  return (busy.length > 0 ? busy : sessions).slice(0, AMBIENT_CATS);
}
