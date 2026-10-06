import { useCallback, useEffect, useRef, useState } from "react";
import { events } from "@/lib/bindings";

/**
 * A list loaded from Rust that reloads when `watch` fires (the clipboard
 * watcher's event) and when its inputs change. A late answer to an old query
 * never overwrites a newer one.
 */
export function useLoaded<T>(
  load: () => Promise<T | null>,
  deps: unknown[],
  watch = false,
): { data: T | null; reload: () => void } {
  const [data, setData] = useState<T | null>(null);
  const latest = useRef(0);
  const run = useCallback(() => {
    const ticket = ++latest.current;
    void load().then((next) => {
      if (ticket === latest.current && next !== null) setData(next);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(run, [run]);
  useEffect(() => {
    if (!watch) return;
    let off: (() => void) | undefined;
    let gone = false;
    void events.clipboardChanged.listen(run).then((fn) => (gone ? fn() : (off = fn)));
    return () => {
      gone = true;
      off?.();
    };
  }, [watch, run]);
  return { data, reload: run };
}
