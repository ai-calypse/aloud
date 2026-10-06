import { useState } from "react";
import { XIcon } from "lucide-react";
import type { Island, IslandAction } from "@/lib/bindings";
import { AppIcon as GlyphIcon } from "@/components/notch/app-icon";
import { Whisper } from "@/features/notch/IslandPill";
import {
  capsuleText,
  dwellMs,
  isCapsule,
  islandRole,
  islandTone,
  opensOnHover,
  takeoverTier,
} from "@/features/notch/islands";
import { Nudge, isNudge } from "./Nudge";
import { useDwell } from "./useDwell";

/** What the toast is drawn as: see `toastForm`. */
export type ToastForm = "capsule" | "peek" | "card";

/**
 * The size a takeover is drawn at. A peek or card the gate shrank (the user is
 * typing) is a side capsule until the pointer opens it into what it really is.
 */
export function toastForm(island: Island, hovering: boolean): ToastForm {
  if (isCapsule(island) && !(hovering && opensOnHover(island))) return "capsule";
  return takeoverTier(island) === "card" ? "card" : "peek";
}

/**
 * A takeover that is bigger than a whisper, hanging from the notch. It is the
 * notch panel's own content, so it grows out of the cutout — or the run's panel
 * shrinks into it — on the panel's spring, and leaves the same way (`onLeave`
 * plays the collapse before anything unmounts).
 *
 * Three sizes, by what it costs to miss: a side capsule (shrunk, because you
 * are typing), a peek (one extra row, 4 s, opens on hover) and a card (sticky,
 * buttons — a call, a battery about to die, an action that failed).
 */
export function NotchToast({
  island,
  hovering,
  onLeave,
  onAction,
  side,
  waiting = 0,
}: {
  island: Island;
  /** The pointer is over the notch; the takeover waits while it is. */
  hovering: boolean;
  onLeave: () => void;
  /** Runs a button through the action registry; true when it did. */
  onAction: (action: IslandAction) => Promise<boolean>;
  /**
   * A whisper riding this takeover: it takes a peek's top row and an echo
   * under a card, and the takeover itself stays put.
   */
  side?: Island | null;
  /** Takeovers waiting their turn: "+N". */
  waiting?: number;
}) {
  // The action that did not work, so its button can say so and stay.
  const [failed, setFailed] = useState<string | null>(null);
  useDwell(dwellMs(island), hovering, onLeave);
  const form = toastForm(island, hovering);
  const tone = islandTone(island);
  const role = islandRole(island);

  const run = (action: IslandAction) =>
    void onAction(action).then((done) => (done ? onLeave() : setFailed(action.action)));
  const button = (action: IslandAction) => (
    <button
      key={action.action}
      className="app-pill"
      disabled={failed === action.action}
      onClick={() => run(action)}
    >
      {failed === action.action ? `Couldn't ${action.label.toLowerCase()}` : action.label}
    </button>
  );

  if (form === "capsule") {
    return <Whisper island={{ ...island, short: island.short ?? capsuleText(island) }} waiting={waiting} />;
  }

  // Another app's notification, an event soon, a reminder due: Nudge.tsx.
  if (isNudge(island)) {
    return (
      <Nudge
        island={island}
        form={form}
        hovering={hovering}
        side={side}
        waiting={waiting}
        onLeave={onLeave}
        button={button}
      />
    );
  }

  if (form === "peek") {
    const [first] = island.actions;
    return (
      <div className="peek" data-open={hovering || undefined} data-tone={tone} data-w-role={role} role="status">
        {/* The top row is the notch's own: the app on one side of the camera,
            what to do about it on the other. A whisper takes it over. */}
        <div className="peek__top">
          {side ? (
            <Whisper island={side} waiting={waiting} />
          ) : (
            <>
              <div className="peek__side">
                <GlyphIcon icon={island.icon} label={island.detail ?? island.title} size="sm" />
                <span className="peek__app">{island.detail ?? "Aloud"}</span>
              </div>
              <span className="island-pill__camera" aria-hidden />
              <div className="peek__side peek__side--end">
                {waiting > 0 && <span className="island-pill__more">+{waiting}</span>}
                {!hovering && first && button(first)}
                <button className="toast__close" aria-label="Dismiss" onClick={onLeave}>
                  <XIcon size={9} strokeWidth={3} />
                </button>
              </div>
            </>
          )}
        </div>
        <p className="peek__line">
          <b>{island.title}</b>
        </p>
        {hovering && island.actions.length > 0 && (
          <div className="peek__actions">{island.actions.map(button)}</div>
        )}
      </div>
    );
  }

  return (
    <div className="toast" data-tier="card" data-tone={tone} data-w-role={role} role="status">
      <button className="toast__close" aria-label="Dismiss" onClick={onLeave}>
        <XIcon size={9} strokeWidth={3} />
      </button>
      <GlyphIcon icon={island.icon} label={island.detail ?? island.title} size="lg" />
      <p className="toast__text">
        {island.title}
        {island.detail && <span className="toast__detail">{island.detail}</span>}
      </p>
      {island.actions.map(button)}
      {/* A key press while a card is up: a small capsule under it, which never
          replaces the card. */}
      {side && (
        <div className="toast__echo">
          <Whisper island={side} />
        </div>
      )}
    </div>
  );
}
