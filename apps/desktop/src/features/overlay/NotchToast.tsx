import { useEffect, useRef, useState } from "react";
import { XIcon } from "lucide-react";
import type { Island, IslandAction } from "@/lib/bindings";
import { AppIcon } from "@/components/preview/kit";

/** How long an alert stays when nobody is looking at it. */
const TOAST_MS = 4000;

/**
 * Calls `done` once `ms` have passed without `held`. Holding pauses the clock
 * rather than restarting it, so a glance does not buy a whole new four seconds.
 */
function useDwell(ms: number, held: boolean, done: () => void) {
  const left = useRef(ms);
  const latest = useRef(done);
  latest.current = done;
  useEffect(() => {
    if (held) return;
    const started = performance.now();
    const timer = setTimeout(() => latest.current(), Math.max(0, left.current));
    return () => {
      clearTimeout(timer);
      left.current -= performance.now() - started;
    };
  }, [held]);
}

/**
 * An alert island, hanging from the notch: the app it happened in, what was
 * done, and at most one thing to do about it — the chip a run of writes that
 * all worked ends on, and the one way any later alert is drawn. It is the
 * notch panel's own content, so it grows out of the cutout — or the run's
 * panel shrinks into it — on the panel's spring, and leaves the same way
 * (`onLeave` plays the collapse before anything unmounts).
 */
export function NotchToast({
  island,
  hovering,
  onLeave,
  onAction,
}: {
  island: Island;
  /** The pointer is over the notch; the alert waits while it is. */
  hovering: boolean;
  onLeave: () => void;
  /** Runs a button through the action registry; true when it did. */
  onAction: (action: IslandAction) => Promise<boolean>;
}) {
  // The action that did not work, so its button can say so and stay.
  const [failed, setFailed] = useState<string | null>(null);
  useDwell(TOAST_MS, hovering, onLeave);

  return (
    <div className="toast" role="status">
      <button className="toast__close" aria-label="Dismiss" onClick={onLeave}>
        <XIcon size={9} strokeWidth={3} />
      </button>
      <AppIcon app={island.detail ?? island.title} icon={island.icon} />
      <p className="toast__text">{island.title}</p>
      {island.actions.map((action) => (
        <button
          key={action.action}
          className="app-pill"
          disabled={failed === action.action}
          onClick={() =>
            void onAction(action).then((done) => (done ? onLeave() : setFailed(action.action)))
          }
        >
          {failed === action.action ? `Couldn't ${action.label.toLowerCase()}` : action.label}
        </button>
      ))}
    </div>
  );
}
