import { useCallback, useEffect, useRef, useState } from "react";
import { commands, events, type NowPlaying, type Reading } from "@/lib/bindings";

/**
 * The Media views' shared ground: the player's state with a clock that keeps
 * running between Rust's polls, and the little pure functions that read it.
 * Polling, parsing and the network are `aloud-media`'s; this is what a view
 * needs to draw them.
 */

/** `m:ss`, or `h:mm:ss` past an hour. Negative and NaN read as zero. */
export function formatTime(ms: number): string {
  const total = Math.floor(Math.max(0, Number.isFinite(ms) ? ms : 0) / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/**
 * Where in the track it is at `at`, given the reading taken at `readAt`.
 * Rust polls once a second; between polls a playing track keeps moving, and a
 * paused one does not. Clamped to the track, which a late poll could outrun.
 */
export function positionAt(now: NowPlaying, readAt: number, at: number): number {
  const elapsed = now.state === "playing" ? Math.max(0, at - readAt) : 0;
  const end = now.durationMs > 0 ? now.durationMs : Number.POSITIVE_INFINITY;
  return Math.min(end, now.positionMs + elapsed);
}

/**
 * The index of the line being sung at `positionMs`: the last one that has
 * started, -1 before the first. `lines` is in time order (Rust sorts it).
 */
export function lineAt(lines: { ms: number }[], positionMs: number): number {
  let lo = 0;
  let hi = lines.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].ms <= positionMs) lo = mid + 1;
    else hi = mid;
  }
  return lo - 1;
}

/** The lowest battery of a device, the one that decides when to charge it. */
export function lowest(batteries: { percent: number }[]): number | null {
  return batteries.length ? Math.min(...batteries.map((b) => b.percent)) : null;
}

const NOTHING: Reading = { now: null, denied: null };

/**
 * The player's state, live. Tells Rust a Media view is on screen (so it
 * polls every second) for as long as the caller is mounted, and returns when
 * each reading arrived so `positionAt` can run the clock between them.
 */
export function useNowPlaying() {
  const [state, setState] = useState<{ reading: Reading; readAt: number }>({
    reading: NOTHING,
    readAt: 0,
  });
  const take = useCallback((reading: Reading) => setState({ reading, readAt: performance.now() }), []);

  useEffect(() => {
    let live = true;
    const accept = (reading: Reading) => live && take(reading);
    let off: Promise<() => void> | undefined;
    try {
      commands.mediaWatch(true).catch(() => {});
      commands.mediaNowPlaying().then(accept).catch(() => {});
      off = events.mediaNowPlaying.listen((e) => accept(e.payload));
    } catch {
      /* browser preview: no Tauri, nothing playing */
    }
    return () => {
      live = false;
      void off?.then((unlisten) => unlisten());
      try {
        commands.mediaWatch(false).catch(() => {});
      } catch {
        /* as above */
      }
    };
  }, [take]);

  /** Shows a press at once; Rust's next reading, ~half a second on, corrects it. */
  const patch = useCallback(
    (change: (now: NowPlaying) => NowPlaying) =>
      setState(({ reading, readAt }) =>
        reading.now
          ? { reading: { ...reading, now: change({ ...reading.now, positionMs: positionAt(reading.now, readAt, performance.now()) }) }, readAt: performance.now() }
          : { reading, readAt },
      ),
    [],
  );

  return { ...state, patch };
}

/** The cover of the track, once Rust has fetched it; null while it has not. */
export function useArtwork(now: NowPlaying | null): string | null {
  const [art, setArt] = useState<{ key: string; uri: string | null } | null>(null);
  const key = now ? `${now.player}|${now.title}|${now.artist}|${Math.floor(now.durationMs / 1000)}` : null;
  const latest = useRef(now);
  latest.current = now;

  useEffect(() => {
    const track = latest.current;
    if (!key || !track) return;
    let live = true;
    try {
      commands
        .mediaArtwork(track)
        .then((uri) => live && setArt({ key, uri }))
        .catch(() => {});
    } catch {
      /* browser preview */
    }
    return () => {
      live = false;
    };
  }, [key]);

  // A stale cover from the last track must never sit under the new title.
  return art && art.key === key ? art.uri : null;
}

/**
 * Loads something on mount and then on a slow clock, for the views whose
 * truth lives in the OS (audio devices, Bluetooth). `refresh` is for after
 * the user changes something.
 */
export function usePolled<T>(load: () => Promise<T>, ms: number, initial: T): [T, () => void] {
  const [value, setValue] = useState(initial);
  const loadRef = useRef(load);
  loadRef.current = load;
  const live = useRef(true);

  const refresh = useCallback(() => {
    try {
      loadRef.current().then((v) => live.current && setValue(v)).catch(() => {});
    } catch {
      /* browser preview: no Tauri */
    }
  }, []);

  useEffect(() => {
    live.current = true;
    refresh();
    const timer = setInterval(refresh, ms);
    return () => {
      live.current = false;
      clearInterval(timer);
    };
  }, [ms, refresh]);

  return [value, refresh];
}
