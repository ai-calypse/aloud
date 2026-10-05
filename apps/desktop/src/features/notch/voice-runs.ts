import { useCallback, useEffect, useState } from "react";
import { commands, type Entry } from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import { timeAgo, truncate } from "@/lib/format";
import { or } from "@/lib/result";

/** Bounded because the list renders every row it fetches, unvirtualised. */
const FETCH = 50;

/** The first line of an outcome: a long answer is one line here, whole when expanded. */
const LINE_CHARS = 110;

export type VoiceRow = {
  id: number;
  kind: Entry["kind"];
  /** What was said. */
  said: string;
  /** One line of how it ended: the paste, the answer, or the error. */
  result: string;
  /** A failed run: shown as the word "Failed", never by colour alone. */
  failed: boolean;
  /** The front app it ran in, if there was one. */
  app: string | null;
  /** "now", "12m", "3h", "2d". */
  when: string;
  /** The whole outcome, for the expanded row. */
  full: string;
};

/**
 * One history row as Agents › Voice words it. The one place that decides what
 * each kind says, so the two looks cannot drift.
 */
export function voiceRow(entry: Entry, now = Date.now()): VoiceRow {
  const said = entry.rawTranscript.trim();
  const outcome = entry.finalTranscript.trim();
  const base = {
    id: entry.id,
    kind: entry.kind,
    said,
    app: entry.appName,
    when: ago(entry.createdAt, now),
    full: outcome,
  };
  if (entry.kind === "dictation") {
    // Cleanup usually changes a few words; only say what was pasted when it
    // differs from what was said.
    const same = outcome === said;
    return {
      ...base,
      result: same || !outcome ? "Pasted" : `Pasted · ${truncate(outcome, LINE_CHARS)}`,
      failed: false,
    };
  }
  if (!entry.success) {
    return { ...base, result: `Failed · ${truncate(outcome, LINE_CHARS) || "no reason given"}`, failed: true };
  }
  return { ...base, result: truncate(outcome, LINE_CHARS) || "Done", failed: false };
}

/** Days past a day, since `timeAgo` stops at hours. */
function ago(createdAt: number, now: number): string {
  const seconds = Math.max(0, Math.floor(now / 1000 - createdAt));
  return seconds >= 86_400
    ? `${Math.floor(seconds / 86_400)}d`
    : timeAgo(new Date(createdAt * 1000).toISOString(), now);
}

/**
 * Past voice runs, newest first. Read when the view mounts and again whenever
 * the live run ends (`settled` flips): Rust writes the row just before it says
 * the run finished, so the refetch finds it.
 */
export function useVoiceRuns(settled: unknown) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const refresh = useCallback(() => {
    // `invoke` throws synchronously in the browser preview (no Tauri), where
    // the fixtures stand in for the history.
    try {
      // An empty list is the normal first-run state; a failed read shows it too.
      void or(commands.historyVoiceRecent(FETCH), [], "reading voice history").then(setEntries);
    } catch {
      /* browser preview */
    }
  }, []);
  useEffect(refresh, [refresh, settled]);
  const remove = useCallback((id: number) => {
    const forget = () => setEntries((rows) => rows.filter((r) => r.id !== id));
    try {
      // Gone from the list only once it is gone from the file.
      void commands.historyDelete(id).then((result) => result.status === "ok" && forget());
    } catch {
      forget(); // browser preview: the fixtures have no file
    }
  }, []);
  return { entries, setEntries, remove };
}

/**
 * The run to draw as a run: one still working, asking or waiting on a yes, and
 * a finished one whose history row has not arrived yet (or never will — the
 * write is best-effort). Once its row exists the run is a row in the list, so
 * the same answer is never on screen twice.
 */
export function liveRun(run: Run | null, entries: Entry[]): Run | null {
  if (!run) return null;
  const ended = run.finished !== undefined || run.error !== undefined;
  if (!ended) return run;
  const listed = entries.some((e) => e.kind === "command" && e.rawTranscript.trim() === run.heard.trim());
  return listed ? null : run;
}

/**
 * The row to open by default: the run that just ended, so "open in Voice" from
 * the compact panel lands on its answer rather than a collapsed line.
 */
export function justFinished(run: Run | null, entries: Entry[]): number | null {
  if (!run || (run.finished === undefined && run.error === undefined)) return null;
  return (
    entries.find((e) => e.kind === "command" && e.rawTranscript.trim() === run.heard.trim())?.id ??
    null
  );
}

/** The widgets' two small tiles: how much was said today, and how much of the list failed. */
export function voiceStats(entries: Entry[], now = Date.now()): { today: number; failed: number } {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const since = midnight.getTime() / 1000;
  return {
    today: entries.filter((e) => e.createdAt >= since).length,
    failed: entries.filter((e) => e.kind === "command" && !e.success).length,
  };
}
