import { useCallback } from "react";
import { commands, type NowPlaying, type Transport } from "@/lib/bindings";
import { nextSpeed, positionAt } from "../../media";

/** How far the skip buttons jump a web video, which has no previous or next. */
export const SKIP_MS = 10_000;

export type Controls = {
  toggle: () => void;
  previous: () => void;
  next: () => void;
  /** Skips a web video by `by` ms. */
  jump: (by: number) => void;
  seek: (ms: number) => void;
  speed: () => void;
};

/** What a press sends. Errors surface as the next reading not changing. */
function send(now: NowPlaying, transport: Transport) {
  try {
    void commands.mediaControl(now.player, transport, null);
  } catch {
    /* browser preview: nothing to drive */
  }
}

/**
 * The player's buttons, each showing its press at once (`patch`) and then
 * asking Rust; the next reading, about half a second on, corrects a guess.
 * The same commands and the same optimism as the calm Now Playing view.
 */
export function usePlayerControls(
  now: NowPlaying | null,
  readAt: number,
  patch: (change: (n: NowPlaying) => NowPlaying) => void,
): Controls {
  const seek = useCallback(
    (ms: number) => {
      if (!now) return;
      patch((n) => ({ ...n, positionMs: ms }));
      try {
        void commands.mediaSeek(now.player, ms);
      } catch {
        /* browser preview */
      }
    },
    [now, patch],
  );
  return {
    toggle: () => {
      if (!now) return;
      patch((n) => ({ ...n, state: n.state === "playing" ? "paused" : "playing" }));
      send(now, "toggle");
    },
    previous: () => now && send(now, "previous"),
    next: () => now && send(now, "next"),
    jump: (by) => {
      if (!now) return;
      seek(Math.max(0, Math.min(now.durationMs || Infinity, positionAt(now, readAt, performance.now()) + by)));
    },
    seek,
    speed: () => {
      if (!now) return;
      const to = nextSpeed(now.speedPct);
      patch((n) => ({ ...n, speedPct: to }));
      try {
        void commands.mediaSetSpeed(now.player, to);
      } catch {
        /* browser preview */
      }
    },
  };
}
