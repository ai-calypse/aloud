import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { commands, events, type ShelfArea, type ShelfPrefs, type ShelfWatch } from "@/lib/bindings";
import { useNotch } from "../context";

/** `1.4 MB`, the way Finder writes it (decimal units, one digit past 10). */
export function formatBytes(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value >= 10 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

/** `now`, `4m`, `3h`, `2d` — how long ago unix `secs` was. */
export function formatAgo(secs: number, nowMs = Date.now()): string {
  const s = Math.max(0, Math.floor(nowMs / 1000 - secs));
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}

/** How many of these happened in the 24 hours before `nowMs`. */
export function countToday(times: number[], nowMs = Date.now()): number {
  const since = nowMs / 1000 - 86400;
  return times.filter((t) => t >= since).length;
}

/**
 * A list Rust owns, read once and again whenever Rust says that area changed.
 * Without Tauri (the browser preview) the read fails and the list stays empty.
 */
export function useShelfList<T>(read: () => Promise<T[]>, area: ShelfArea, enabled = true) {
  const [items, setItems] = useState<T[] | null>(null);
  // The latest `read`, so the listener subscribes once.
  const latest = useRef(read);
  latest.current = read;

  const refresh = useCallback(async () => {
    try {
      setItems(await latest.current());
    } catch {
      setItems([]);
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    void refresh();
    let off: (() => void) | undefined;
    let gone = false;
    events.shelfChanged
      .listen((e) => {
        if (e.payload === area) void refresh();
      })
      .then((fn) => (gone ? fn() : (off = fn)))
      .catch(() => {});
    return () => {
      gone = true;
      off?.();
    };
  }, [area, enabled, refresh]);

  return { items, refresh };
}

/** The Shelf's on/off rows. `set` resolves with Rust's reason if it could not turn one on. */
export function useShelfPrefs() {
  const [prefs, setPrefs] = useState<ShelfPrefs | null>(null);
  useEffect(() => {
    commands.shelfPrefs().then(setPrefs, () => {});
  }, []);
  const set = useCallback(async (which: ShelfWatch, on: boolean): Promise<string | null> => {
    const r = await commands.shelfSetWatch(which, on);
    if (r.status === "ok") {
      setPrefs(r.data);
      return null;
    }
    // Stays off: re-read what Rust kept.
    commands.shelfPrefs().then(setPrefs, () => {});
    return r.error;
  }, []);
  return { prefs, set };
}

export const thumbs = new Map<string, Promise<string | null>>();

/**
 * A picture's `data:` thumbnail, from Rust. Cached by path for the session:
 * a thumbnail only changes when the file does, and a path that did gets a
 * new name in practice (screenshots are never rewritten in place).
 */
export function useThumb(path: string, wanted: boolean): string | null {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    let job = thumbs.get(path);
    if (!job) {
      job = commands.shelfThumbnail(path).then((r) => (r.status === "ok" ? r.data : null));
      thumbs.set(path, job);
    }
    void job.then((uri) => live && setSrc(uri));
    return () => {
      live = false;
    };
  }, [path, wanted]);
  return src;
}

/** A transient message under a list — an error, or what just happened. */
export function useNote(ms = 6000) {
  const [note, setNote] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const show = useCallback(
    (text: string | null) => {
      clearTimeout(timer.current);
      setNote(text);
      if (text) timer.current = setTimeout(() => setNote(null), ms);
    },
    [ms],
  );
  return [note, show] as const;
}

/**
 * Files dropped from Finder onto an open Shelf view go on the shelf. The
 * webview's own `ondrop` never fires for OS drops in Tauri, so this is the
 * native event; it hands over paths, not bytes. Returns whether a drag is
 * over the panel, for the view to say "release".
 */
export function useDropToShelf(onNote: (text: string | null) => void): boolean {
  const { navigate } = useNotch();
  const [over, setOver] = useState(false);
  const latest = useRef({ navigate, onNote });
  latest.current = { navigate, onNote };

  useEffect(() => {
    let off: (() => void) | undefined;
    let gone = false;
    try {
      getCurrentWebview()
        .onDragDropEvent(async ({ payload }) => {
          if (payload.type === "enter" || payload.type === "over") return setOver(true);
          setOver(false);
          if (payload.type !== "drop" || payload.paths.length === 0) return;
          const r = await commands.shelfAdd(payload.paths);
          if (r.status === "error") return latest.current.onNote(r.error);
          latest.current.onNote(
            r.data.failed.length > 0 ? `Could not shelve ${r.data.failed.length}: ${r.data.failed[0]}` : null,
          );
          latest.current.navigate("shelf", "files");
        })
        .then((fn) => (gone ? fn() : (off = fn)))
        .catch(() => {});
    } catch {
      /* browser preview: no webview */
    }
    return () => {
      gone = true;
      off?.();
    };
  }, []);

  return over;
}
