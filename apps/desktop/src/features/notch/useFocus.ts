import { useEffect, useState } from "react";
import { commands, events, type FocusSnapshot, type FocusStats } from "@/lib/bindings";
import { nextTickIn } from "@/lib/focus";

/**
 * The timer and keep-awake, live. Rust owns the state and pushes a snapshot
 * on every change; between pushes the clock is `now`, re-read only when the
 * face would change (and only while something runs), so an idle Focus tab
 * costs nothing.
 */
export function useFocus(): { snap: FocusSnapshot | null; now: number } {
  const [snap, setSnap] = useState<FocusSnapshot | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let live = true;
    // Browser preview has no Tauri: the view then shows its idle self.
    commands.focusState().then((s) => live && setSnap(s)).catch(() => {});
    const off = events.focus.listen((e) => {
      setSnap(e.payload);
      setNow(Date.now());
    });
    return () => {
      live = false;
      void off.then((f) => f()).catch(() => {});
    };
  }, []);

  const running = snap?.timer.status === "running";
  useEffect(() => {
    if (!snap || !running) return;
    const id = window.setTimeout(() => setNow(Date.now()), nextTickIn(snap.timer, Date.now()));
    return () => window.clearTimeout(id);
  }, [snap, running, now]);

  return { snap, now };
}

/** Streak and week. Refetched when a session lands, which is when `sessionsSeen` moves. */
export function useFocusStats(refetchOn: unknown): FocusStats | null {
  const [stats, setStats] = useState<FocusStats | null>(null);
  useEffect(() => {
    let live = true;
    commands
      .focusStats()
      .then((r) => live && r.status === "ok" && setStats(r.data))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [refetchOn]);
  return stats;
}
