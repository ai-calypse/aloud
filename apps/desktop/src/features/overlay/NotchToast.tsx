import { useEffect, useRef, useState } from "react";
import { XIcon } from "lucide-react";
import { commands, type Toast } from "@/lib/bindings";
import { AppIcon } from "@/components/preview/kit";

/** How long a toast stays when nobody is looking at it. */
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
 * The confirmation chip a run of writes that all worked ends on, hanging from
 * the notch: the app it acted in, what was done, and at most one thing to do
 * about it. It is the notch panel's own content, so it grows out of the
 * cutout — or the run's panel shrinks into it — on the panel's spring, and
 * leaves the same way (`onLeave` plays the collapse before anything unmounts).
 */
export function NotchToast({
  toast,
  hovering,
  onLeave,
}: {
  toast: Toast;
  /** The pointer is over the notch; the toast waits while it is. */
  hovering: boolean;
  onLeave: () => void;
}) {
  const [undoFailed, setUndoFailed] = useState(false);
  useDwell(TOAST_MS, hovering, onLeave);
  const action = toast.action;

  return (
    <div className="toast" role="status">
      <button className="toast__close" aria-label="Dismiss" onClick={onLeave}>
        <XIcon size={9} strokeWidth={3} />
      </button>
      <AppIcon app={toast.app} icon={toast.icon} />
      <p className="toast__text">{toast.text}</p>
      {action?.kind === "show" && (
        <button
          className="app-pill"
          onClick={() => {
            void commands.showResult(action.app, action.target);
            onLeave();
          }}
        >
          Show
        </button>
      )}
      {action?.kind === "undo" && (
        <button
          className="app-pill"
          disabled={undoFailed}
          onClick={() =>
            void commands.undoTidy(action.folder).then((res) => {
              if (res.status === "ok") onLeave();
              else setUndoFailed(true);
            })
          }
        >
          {undoFailed ? "Couldn't undo" : "Undo"}
        </button>
      )}
    </div>
  );
}
