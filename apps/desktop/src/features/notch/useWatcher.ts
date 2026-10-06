import { useCallback, useEffect, useState } from "react";
import { commands, type Watcher } from "@/lib/bindings";

/**
 * A Mac watcher's on/off, owned by Rust (it runs while the notch is closed).
 * `null` until read, so the switch is not drawn in a state it may not be in.
 */
export function useWatcher(watcher: Watcher): [boolean | null, (on: boolean) => void] {
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    commands.macWatchers().then((w) => setOn(w[watcher] ?? true), () => {});
  }, [watcher]);

  const set = useCallback(
    (next: boolean) => {
      setOn(next);
      // Back to what Rust has if it could not save the choice.
      commands.setMacWatcher(watcher, next).then(
        (res) => res.status === "error" && setOn(!next),
        () => setOn(!next),
      );
    },
    [watcher],
  );

  return [on, set];
}
