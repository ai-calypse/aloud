import React, { useEffect, useRef, useState } from "react";
import type { AgentPermission, AgentSession, ApprovalReply } from "@/lib/bindings";
import {
  agentStage,
  agentHue,
  stageCat,
  stageSeed,
  STAGE_LABEL,
  type AgentStage,
} from "@/lib/agent-stage";
import { createSpring } from "@/lib/spring";
import { AgentDetailBody } from "./AgentList";
import {
  AgentAvatar,
  AGENT_NAME,
  rowState,
  rowName,
  type PeekFn,
} from "./agentIdentity";

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

/** How many stage cats the resting strip shows before collapsing to a `+N`. */
const AMBIENT_CATS = 4;

/**
 * The coding agents' face in the closed notch: a tight stack of stage cats,
 * drawn as the lead of the `agents` island left of the camera (see
 * `IslandPill`). It does NOT expand in place — hovering grows the notch down
 * and *deals* the same cats out into a row of bigger circles (`NotchAgents`,
 * dealing from the stack's side via `--from`).
 *
 * It used to be a strip of its own on the canvas beside the cutout, reporting
 * its own click-box. As an island it is the pill's content, so the pill's
 * measured box is the click-box and there is one writer of it, not two.
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
 * The cats the resting strip (and, one-to-one, the dealt row) show: the busy
 * agents, or — if none are busy but one is asking — that one, so the sign has a
 * face to ring. Capped at `AMBIENT_CATS`. Shared so the strip and the row agree
 * on exactly which cats exist.
 */
function ambientCats(sessions: AgentSession[]): AgentSession[] {
  const busy = sessions.filter((s) => s.busy);
  return (busy.length > 0 ? busy : sessions).slice(0, AMBIENT_CATS);
}

/**
 * Deal tuning, chosen on the mockup at
 * claude.ai/code/artifact/e0d1601a-a6e3-453e-8ee2-f64e172e3244 (layout B).
 * `SPACING` is the gap between dealt circles' centres; `STAGGER` the beat
 * between each card being flicked out — small, so the row deals as a quick
 * ripple rather than one-at-a-time. `520ms` is the per-card travel, set in CSS.
 */
const DEAL_SPACING = 92;

const DEAL_STAGGER = 40;

/** The dealt card's width — matches `.notch-card` in overlay.css. Sizes the
 *  deal layer so the panel measures the full row (absolute cards do not). */
const DEAL_CARD_W = 86;

/** How long the cursor must rest on one card before the plate unfolds the full
 *  detail. Long enough that a sweep across the row never triggers it. */
const DWELL_MS = 500;

/**
 * The agents dealt into the notch, shown while the resting notch is hovered.
 * The same cats the resting strip shows, but bigger and circular — they deal
 * out from the strip's stacked position into a straight row (layout B), each
 * flicked a beat after the last, like a dealer going round the table. One
 * card per agent: the cat, its status underneath. Hovering a card lifts it;
 * clicking opens that agent's detail. "View all agents" opens the full list.
 *
 * The deal is pure CSS: each card starts collapsed on the resting stack
 * (`--from`, left of the cutout) and transitions to `translateX(--tx)` with a
 * per-card `transition-delay`. Mount plays it once (the notch only mounts this
 * on hover), armed by flipping `data-dealt` a frame after mount so the cards
 * have a from-state to leave.
 */
export function NotchAgents({
  sessions,
  permissions,
  onOpenAgent,
  onOpenAgents,
  onResolve,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  onOpenAgent: (id: string) => void;
  onOpenAgents: () => void;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  const cats = ambientCats(sessions);
  // Deal on mount: `dealt` is false for the first paint (cards sit on the
  // stack), true the next frame (they deal out). A ref-free flag driven by one
  // rAF — no per-frame state, just the single flip the transition needs.
  const [dealt, setDealt] = useState(false);
  // Which card the cursor is on, or null. Drives the shared detail plate below
  // the row — one plate, not one per card, so gliding across just re-keys its
  // contents rather than opening and closing three separate surfaces.
  const [hovered, setHovered] = useState<string | null>(null);
  // Progressive reveal: a quick hover shows the compact plate (+ live "Now"
  // line); *dwelling* on one card past `DWELL_MS` unfolds the full detail
  // (cwd, prompt, subagents). Sweeping the row peeks; lingering commits to
  // reading — without a click. Reset whenever the hovered card changes.
  const [dwelling, setDwelling] = useState(false);
  useEffect(() => {
    const id = requestAnimationFrame(() => setDealt(true));
    return () => cancelAnimationFrame(id);
  }, []);
  useEffect(() => {
    setDwelling(false);
    if (!hovered) return;
    const t = setTimeout(() => setDwelling(true), DWELL_MS);
    return () => clearTimeout(t);
  }, [hovered]);
  if (cats.length === 0) {
    return <p className="notch-peek--empty">No agents running</p>;
  }
  const mid = (cats.length - 1) / 2;
  // The deal layer must be sized *explicitly*: its cards are absolutely
  // positioned, so they contribute nothing to the content box the drop-pill
  // spring measures — without this the panel collapses to the button's width
  // and the dealt row overflows a too-narrow notch. Width spans edge card to
  // edge card; height fits a card + its label.
  const dealWidth = (cats.length - 1) * DEAL_SPACING + DEAL_CARD_W;
  // Only scan when something is actually hovered; the height is a constant the
  // CSS owns, so only the derivable width is set inline here.
  const hoveredSession = hovered ? (cats.find((s) => s.id === hovered) ?? null) : null;
  return (
    <div className="notch-agents" data-dealt={dealt || undefined}>
      <div className="notch-deal" style={{ width: dealWidth }}>
        {cats.map((session, i) => (
          <NotchCard
            key={session.id}
            session={session}
            asking={askingIds.has(session.id)}
            tx={(i - mid) * DEAL_SPACING}
            stagger={i * DEAL_STAGGER}
            onOpen={() => onOpenAgent(session.id)}
            // A leave clears only if this card still owns the hover — the
            // browser fires the next card's enter before this leave, so a bare
            // `null` would wipe the freshly-hovered card.
            onHover={(on) =>
              setHovered((h) => (on ? session.id : h === session.id ? null : h))
            }
          />
        ))}
      </div>
      {/* The peek under the row for the hovered card: a compact head on a quick
          hover, and — once dwelt on past DWELL_MS — the *same* full detail view
          a click opens (`AgentDetailBody`), so you can read what an agent is
          doing without clicking in. One shared surface; gliding re-keys it. */}
      <NotchPeekPlate
        session={hoveredSession}
        permissions={permissions}
        dwelling={dwelling}
        onResolve={onResolve}
      />
      <button className="notch-agents__all" type="button" onClick={onOpenAgents}>
        View all agents
      </button>
    </div>
  );
}

/**
 * The hover pop's spring. Matches the app's pop character (motion.css
 * `--spring-pop`, damping 0.62 → ~8% overshoot — the little rebound that reads
 * as landing), on a snappy response because it fires every time the cursor
 * crosses a card. `response` is seconds, per `lib/spring.ts`.
 */
const POP_DAMPING = 0.62;

const POP_RESPONSE = 0.28;

/**
 * One dealt card: the meme cat (its *stage*), inside a per-agent colour ring
 * with a stage-state badge in the corner, and `project @ branch` underneath —
 * so which agent it is, and which one you are about to click, reads before any
 * hover. The cat alone could not carry that: it is keyed to the stage, which is
 * transient and identical across two agents doing the same thing.
 *
 * The card also has a *live* hover pop. The pop can be redirected mid-flight —
 * glide from one cat to the next and the first retracts while the second grows,
 * reversible halfway without stalling — which is exactly what `lib/spring.ts`
 * is for (a baked CSS curve retargeted halfway restarts from zero velocity; the
 * live spring carries its momentum through). A spring drives a `--pop` value
 * 0→1 written straight onto the node each frame (no React state between
 * frames); CSS turns it into a scale+lift on the profile image. The image, not
 * the card: the card's transform is owned by the deal, so layering the pop on
 * the image keeps the two from fighting.
 */
function NotchCard({
  session,
  asking,
  tx,
  stagger,
  onOpen,
  onHover,
}: {
  session: AgentSession;
  asking: boolean;
  tx: number;
  stagger: number;
  onOpen: () => void;
  /** Cursor entered (true) or left (false) this card — drives the shared plate. */
  onHover: (on: boolean) => void;
}) {
  const stage = agentStage(session, asking);
  const seed = stageSeed(session.id, stage);
  const state = rowState(session);
  const el = useRef<HTMLButtonElement>(null);
  const pop = useRef<ReturnType<typeof createSpring> | null>(null);
  useEffect(() => {
    const node = el.current;
    if (!node) return;
    const spring = createSpring((value) => node.style.setProperty("--pop", String(value)), {
      damping: POP_DAMPING,
      response: POP_RESPONSE,
    });
    pop.current = spring;
    return () => spring.stop();
  }, []);
  return (
    <button
      ref={el}
      className="notch-card"
      type="button"
      data-asking={asking || undefined}
      title={rowName(session)}
      // Only the genuinely per-card values live here: the dealt slot `--tx`, the
      // identity hue `--ring`, and the deal's stagger. `--from` (where it deals
      // from) is identical for every card, so it lives in the CSS rule.
      style={
        {
          "--tx": `${tx}px`,
          "--ring": `hsl(${agentHue(session.id)} 70% 58%)`,
          transitionDelay: `${stagger}ms`,
        } as React.CSSProperties
      }
      onMouseEnter={() => {
        pop.current?.to(1);
        onHover(true);
      }}
      onMouseLeave={() => {
        pop.current?.to(0);
        onHover(false);
      }}
      onClick={onOpen}
    >
      <span className="notch-card__disc">
        <AgentAvatar
          className="notch-card__cat"
          stage={stage}
          seed={seed}
          hue={agentHue(session.id)}
        />
        {/* The stage badge: what it is doing right now, colour-coded the same
            way the agent-list rows are (busy/live/done, orange when asking). */}
        <span
          className="notch-card__badge"
          data-state={asking ? "asking" : state}
          aria-hidden
        />
      </span>
      {/* Identity, not stage: project @ branch, so the card says *which* agent. */}
      <span className="notch-card__label">{rowName(session)}</span>
    </button>
  );
}

/**
 * The peek under the dealt row for the hovered card — read an agent without a
 * click, in two tiers:
 *
 * - A quick hover shows a compact head: `project @ branch`, the stage, the
 *   agent · model · subagent count, and the live "Now" tool.
 * - *Dwelling* past `DWELL_MS` swaps that for the **same full detail view a
 *   click opens** (`AgentDetailBody`: avatar, cwd, the "You:" prompt, the
 *   approve/deny card if it is asking, Now, and every subagent) — the identical
 *   component and width, so the peek is the real thing, just without the click.
 *   Clicking still opens the pinned version that survives mouse-leave.
 *
 * One shared surface for the whole row: gliding re-keys it. It holds the last
 * session through the collapse so the fold never blanks mid-way; and both tiers
 * are pinned to the detail's own width (a `min-width`), so the peek is always
 * the notch-panel size, never as wide as its longest line of text.
 */
function NotchPeekPlate({
  session,
  permissions,
  dwelling,
  onResolve,
}: {
  session: AgentSession | null;
  permissions: AgentPermission[];
  /** The cursor has rested on this card long enough to show the full detail. */
  dwelling: boolean;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  // Hold the last session through the collapse, so it does not blank out while
  // the plate is still folding away.
  const shown = useRef<AgentSession | null>(null);
  if (session) shown.current = session;
  const s = shown.current;
  const permission = s ? (permissions.find((p) => p.sessionId === s.id) ?? null) : null;
  const asking = permission !== null;
  const stage = s ? agentStage(s, asking) : "idle";
  const state = s ? rowState(s) : "done";
  const subs = s?.subagents.length ?? 0;
  // Full detail only once the cursor has settled on a card — never mid fold-away
  // (no session), or it would grow as it is trying to leave.
  const expanded = dwelling && session !== null;
  return (
    <div
      className="notch-plate"
      data-open={session ? "" : undefined}
      data-expanded={expanded ? "" : undefined}
      aria-hidden={!session}
    >
      {s &&
        (expanded ? (
          // The real detail view a click opens — no back button (nowhere to go),
          // and no cat-peek inside a peek.
          <div className="notch-plate__inner notch-plate__inner--full" key={`${s.id}-full`}>
            <AgentDetailBody
              session={s}
              permission={permission}
              onResolve={onResolve}
              onPeek={NO_PEEK}
            />
          </div>
        ) : (
          <div className="notch-plate__inner" key={s.id}>
            <span className="notch-plate__title">{rowName(s)}</span>
            {/* What it is doing right now, led by the badge's own status dot. */}
            <span className="notch-plate__stage">
              <span className="notch-plate__pip" data-state={asking ? "asking" : state} aria-hidden />
              {STAGE_LABEL[stage]}
            </span>
            <span className="notch-plate__meta">
              {AGENT_NAME[s.agent] ?? s.agent}
              {s.model && <span className="notch-plate__dot"> · {s.model}</span>}
              {subs > 0 && (
                <span className="notch-plate__dot">
                  {" "}
                  · {subs} subagent{subs === 1 ? "" : "s"}
                </span>
              )}
            </span>
            <span className="notch-plate__now">
              <span className="notch-plate__now-label">Now</span>
              <span className="notch-plate__now-tool" data-idle={!s.tool || undefined}>
                {s.tool ?? (s.busy ? "thinking…" : "idle")}
              </span>
            </span>
          </div>
        ))}
    </div>
  );
}

/** The hover peek shows the detail inline; the enlarged cat-peek does not fire
 *  inside it, so `AgentDetailBody`'s avatar gets a no-op peek. */
const NO_PEEK: PeekFn = () => {};
