import { useEffect, useState } from "react";

/**
 * True once `on` has held for `ms` without a break, and false the instant it
 * lets go. The pointer passing through the notch on its way to the menu bar
 * never gets as far as `ms`, so it never opens anything.
 */
export function useHoverIntent(on: boolean, ms: number): boolean {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (!on) {
      setReady(false);
      return;
    }
    const timer = setTimeout(() => setReady(true), ms);
    return () => clearTimeout(timer);
  }, [on, ms]);
  return on && ready;
}
