import { useEffect, useRef } from "react";

/**
 * Calls `done` once `ms` have passed without `held`. Holding pauses the clock
 * rather than restarting it, so a glance does not buy a whole new four seconds.
 * `ms` null never calls it: a card stays until answered. A new `key` is a new
 * takeover and gets its full time.
 */
export function useDwell(ms: number | null, held: boolean, done: () => void, key?: unknown) {
  const left = useRef(ms ?? 0);
  const latest = useRef(done);
  latest.current = done;
  // Declared first: on a new key the old timer's cleanup has already taken its
  // elapsed time off, and this puts the full time back before the next starts.
  useEffect(() => {
    left.current = ms ?? 0;
  }, [key, ms]);
  useEffect(() => {
    if (held || ms === null) return;
    const started = performance.now();
    const timer = setTimeout(() => latest.current(), Math.max(0, left.current));
    return () => {
      clearTimeout(timer);
      left.current -= performance.now() - started;
    };
  }, [held, ms, key]);
}
