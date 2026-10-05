import type { ReactNode } from "react";
import { XIcon } from "lucide-react";
import type { Island, IslandAction } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import type { WidgetRole } from "@/components/notch/widgets/roles";
import { Whisper } from "@/features/notch/IslandPill";
import { dwellMs } from "@/features/notch/islands";

/**
 * A nudge is another app's notification, an event about to start or a reminder
 * that came due: Rust builds the island (`aloud_notify::island`,
 * `aloud_agenda::soon_island`), and this draws it in the events design's
 * Widgets look — the app's own icon, its name, a bold sender or title and one
 * line, in the app's accent. Drawn the same in both looks: a nudge has no Calm
 * form to fall back to.
 */

const PREFIXES = ["nudge:", "cal:soon:", "rem:due:"];

export const isNudge = (island: Island) => PREFIXES.some((p) => island.id.startsWith(p));

/** Each app's accent, as the design colours them; an app it has no colour for is the agent's indigo. */
const BUNDLE_ROLE: Record<string, WidgetRole> = {
  "com.apple.MobileSMS": "ok",
  "com.apple.mail": "info",
  // The design takes the Podcasts purple as the closest widget hue to Slack's.
  "com.tinyspeck.slackmacgap": "podcast",
  "com.apple.FaceTime": "ok",
  "com.apple.TelephonyUtilities": "ok",
  "com.apple.iCal": "calendar",
  "com.apple.reminders": "orange",
  "com.apple.sharingd": "info",
};

export function nudgeRole(island: Island): WidgetRole {
  if (island.id.startsWith("cal:soon:")) return "calendar";
  if (island.id.startsWith("rem:due:")) return "orange";
  // `nudge:<bundle>:<row>`: a bundle id has dots and no colons.
  return BUNDLE_ROLE[island.id.split(":")[1] ?? ""] ?? "agent";
}

/**
 * The peek or card form of a nudge. `button` is NotchToast's own, so a button
 * that did not work says so in the same place it does everywhere else.
 */
export function Nudge({
  island,
  form,
  hovering,
  side,
  waiting = 0,
  onLeave,
  button,
}: {
  island: Island;
  form: "peek" | "card";
  hovering: boolean;
  side?: Island | null;
  waiting?: number;
  onLeave: () => void;
  button: (action: IslandAction) => ReactNode;
}) {
  const role = nudgeRole(island);
  const app = island.detail ?? "Aloud";
  const icon = <AppIcon icon={island.icon} label={app} size="sm" />;
  const close = (
    <button className="toast__close" aria-label="Dismiss" onClick={onLeave}>
      <XIcon size={9} strokeWidth={3} />
    </button>
  );

  if (form === "card") {
    return (
      <div className="toast nudge nudge--card" data-tier="card" data-w-role={role} role="status">
        <div className="nudge__head">
          <div className="peek__side">
            {icon}
            <span className="peek__app">{app}</span>
          </div>
          <span className="island-pill__camera" aria-hidden />
          <div className="peek__side peek__side--end">{close}</div>
        </div>
        <div className="nudge__who">
          <span className="nudge__avatar" aria-hidden>
            {island.title.trim().charAt(0).toUpperCase()}
          </span>
          <p className="nudge__text">
            <b>{island.title}</b>
            {island.body && <span>{island.body}</span>}
          </p>
        </div>
        <div className="nudge__acts">{island.actions.map(button)}</div>
        {/* A key press while a call is up: a capsule under it, never replacing it. */}
        {side && (
          <div className="toast__echo">
            <Whisper island={side} />
          </div>
        )}
      </div>
    );
  }

  const dwell = dwellMs(island);
  const [first] = island.actions;
  // Join is the point of a calendar peek, so it stays in the top row; every
  // other nudge keeps that row clean and shows its button on hover.
  const inRow = first?.action === "calendar.join";
  return (
    <div className="peek nudge" data-open={hovering || undefined} data-w-role={role} role="status">
      <div className="peek__top">
        {side ? (
          <Whisper island={side} waiting={waiting} />
        ) : (
          <>
            <div className="peek__side">
              {icon}
              <span className="peek__app">{app}</span>
            </div>
            <span className="island-pill__camera" aria-hidden />
            <div className="peek__side peek__side--end">
              {waiting > 0 && <span className="island-pill__more">+{waiting}</span>}
              {inRow && !hovering && button(first)}
              {close}
            </div>
          </>
        )}
      </div>
      <p className="peek__line nudge__line">
        <b>{island.title}</b>
        {island.body && <span className="nudge__body">{island.body}</span>}
      </p>
      {hovering && island.actions.length > 0 && <div className="peek__actions">{island.actions.map(button)}</div>}
      {dwell !== null && (
        <span className="nudge__dwell" style={{ animationDuration: `${dwell}ms` }} aria-hidden />
      )}
    </div>
  );
}
