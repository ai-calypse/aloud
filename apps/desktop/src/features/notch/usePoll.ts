import { useEffect, useRef, useState } from "react";

/**
 * Calls `load` now and every `ms` while the component is mounted, which for a
 * notch view is while it is on screen — so a Mac reading costs nothing with
 * the panel closed or on another tab. Skips a tick while the webview is
 * hidden, and never overlaps itself when a read is slower than the interval
 * (stats wait a quarter second between CPU samples).
 *
 * `undefined` until the first read lands; a failed read (the browser preview
 * has no Tauri) leaves the last value.
 */
export function usePoll<T>(load: () => Promise<T>, ms: number): T | undefined {
  const [value, setValue] = useState<T>();
  // The latest closure without restarting the interval each render.
  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    let live = true;
    let busy = false;
    const tick = () => {
      if (busy || document.visibilityState === "hidden") return;
      busy = true;
      loadRef
        .current()
        .then((next) => live && setValue(next))
        .catch(() => {})
        .finally(() => {
          busy = false;
        });
    };
    tick();
    const timer = setInterval(tick, ms);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [ms]);

  return value;
}
