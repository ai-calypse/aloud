import { useCallback, useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, events, type Entry, type Span } from "@/lib/bindings";
import { or } from "@/lib/result";
import { formatDate, formatDuration } from "@/lib/format";
import { spanFields } from "@/lib/waterfall";
import { Button } from "@/components/ui/button";
import { Group, GridRow, PaneTitle } from "@/components/ui/rows";

/** Bounded because the pane renders every row it fetches, unvirtualised. */
const FETCH = 200;

const COLUMNS = "150px 96px 52px 1fr auto";

/**
 * Every dictation that survived retention, newest first. Expanding joins the
 * row to its `timings` spans on `utteranceId` to show the pipeline that
 * produced it.
 */
export function RunLogView() {
  const [entries, setEntries] = useState<Entry[]>([]);
  // Fetched whole rather than per row: `recent_timings` has no by-id form.
  const [spans, setSpans] = useState<Map<number, Span[]>>(new Map());
  /** Oldest utterance the spans fetch reached. Rows older than this were not
      loaded, which the UI must word differently from having been pruned. */
  const [spansFrom, setSpansFrom] = useState<number | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [retrying, setRetrying] = useState<number | null>(null);
  /** Per-row retry message. Never a toast: a rate-limit that vanishes before
      it is read is the one that had to be kept. */
  const [note, setNote] = useState<{ id: number; text: string } | null>(null);

  const refresh = useCallback(() => {
    // An empty log is the normal first-run state, so a read failure shows the
    // empty state rather than taking the pane down.
    or(commands.historyRecent(FETCH), [], "reading the run log").then(
      setEntries,
    );
    or(commands.recentTimings(FETCH), [], "reading run breakdowns").then(
      (rows) => {
        const byId = new Map<number, Span[]>();
        for (const row of rows) {
          const list = byId.get(row.utteranceId);
          if (list) list.push(row);
          else byId.set(row.utteranceId, [row]);
        }
        setSpans(byId);
        setSpansFrom(rows.length ? Math.min(...rows.map((r) => r.utteranceId)) : null);
      },
    );
  }, []);

  useEffect(() => {
    refresh();
    // The row is written as the utterance span closes, just after the phase is
    // emitted — hence the delay.
    const unlisten = events.dictationPhase.listen((event) => {
      if (["done", "error", "idle"].includes(event.payload)) {
        setTimeout(refresh, 150);
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, [refresh]);

  async function remove(id: number) {
    const result = await commands.historyDelete(id);
    if (result.status === "error") {
      console.error(`deleting run ${id}: ${result.error}`);
      return;
    }
    setEntries((rows) => rows.filter((r) => r.id !== id));
  }

  async function clearAll() {
    const result = await commands.historyClear();
    if (result.status === "error") {
      console.error(`clearing the run log: ${result.error}`);
      return;
    }
    setEntries([]);
  }

  function copy(entry: Entry) {
    navigator.clipboard.writeText(entry.finalTranscript).then(() => {
      setCopied(entry.id);
      setTimeout(() => setCopied(null), 1200);
    });
  }

  /**
   * Re-runs cleanup on the stored raw transcript. `testPrompt` rather than a
   * second cleanup command — a parallel path is the one that quietly stops
   * matching the real pipeline.
   */
  async function retry(entry: Entry) {
    setRetrying(entry.id);
    setNote(null);
    // Read now, not at mount: a retry usually follows editing the prompt.
    const settings = await commands.getSettings();
    const run = await commands.testPrompt(
      settings.customSystemPrompt ?? "",
      entry.rawTranscript,
    );
    setRetrying(null);

    if (run.status === "error") {
      // Covers the MIN_WORDS trap: cleanup short-circuits on a short
      // transcript, so the retry reads as a button that does nothing.
      setNote({ id: entry.id, text: run.error });
      return;
    }

    // An empty result is the EMPTY sentinel asking for a blank paste. Writing
    // it back would erase the user's transcript, and there is no undo.
    if (run.data.trim() === "") {
      setNote({
        id: entry.id,
        text: "The prompt judged this to be filler and asked for an empty paste, so the transcript was left alone.",
      });
      return;
    }

    const written = await commands.historyUpdateFinal(entry.id, run.data);
    if (written.status === "error") {
      setNote({ id: entry.id, text: written.error });
      return;
    }
    setEntries((rows) =>
      rows.map((r) =>
        r.id === entry.id ? { ...r, finalTranscript: run.data } : r,
      ),
    );
    setNote({
      id: entry.id,
      text:
        run.data === entry.finalTranscript
          ? "Cleanup produced the same text as before."
          : "Updated.",
    });
  }

  /**
   * The whole pipeline from the WAV down, unlike `retry` which starts from the
   * words already in the row. A null engine means "whatever Settings says";
   * switching engines can trigger a 150MB model download, so the long busy
   * state is not a hang — hence the wording below.
   */
  async function retryTranscription(entry: Entry, engine: "local" | "cloud" | null) {
    setRetrying(entry.id);
    setNote({
      id: entry.id,
      text: "Re-transcribing. Switching engines loads the other model first, which can take a while the first time.",
    });
    const run = await commands.historyRetryTranscription(entry.id, engine);
    setRetrying(null);

    if (run.status === "error") {
      setNote({ id: entry.id, text: run.error });
      return;
    }
    // Refetched, not patched: `rawTranscript` changed too, and a half-updated
    // row would show the old "Heard" beside the new "Pasted".
    refresh();
    setNote({ id: entry.id, text: "Re-transcribed." });
  }

  /**
   * ponytail: export is a clipboard write, not a file. A `<a download>` blob
   * needs a download handler the webview does not have, and a Save dialog
   * means `tauri-plugin-dialog` + `-fs` for one button. Add them if anyone
   * ever wants this without a paste in between.
   */
  function exportAll() {
    navigator.clipboard.writeText(JSON.stringify(entries, null, 2)).then(() => {
      setCopied(-1);
      setTimeout(() => setCopied(null), 1200);
    });
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <PaneTitle
          title="Run Log"
          subtitle="Every dictation kept by the History setting. Expand a run to see the pipeline that produced it."
        />
        {entries.length > 0 && (
          <div className="flex shrink-0 gap-4 pt-1">
            <Button variant="plain" size="text" onClick={exportAll}>
              {copied === -1 ? "Copied" : "Export"}
            </Button>
            <Button variant="plain-danger" size="text" onClick={clearAll}>
              Clear all
            </Button>
          </div>
        )}
      </div>

      <Group>
        {entries.length === 0 ? (
          <div className="py-8 text-center text-ui text-ink-2">
            <p className="mb-1 text-[15px] font-semibold text-foreground">
              No Runs Yet
            </p>
            <p>Dictate something and it will show up here.</p>
          </div>
        ) : (
          entries.map((entry) => {
            const open = expanded === entry.id;
            return (
              <div
                key={entry.id}
                className="border-b-[0.5px] border-separator last:border-b-0"
              >
                <GridRow columns={COLUMNS} className="border-b-0 py-[13px]">
                  <span className="truncate text-hint tabular-nums text-ink-2">
                    {formatDate(entry.createdAt)}
                  </span>
                  <span className="truncate text-hint text-ink-2">
                    {entry.appName ?? "—"}
                  </span>
                  <span className="text-hint tabular-nums text-ink-2">
                    {formatDuration(entry.durationMs)}
                  </span>
                  <span className="truncate text-row">
                    {entry.finalTranscript || "(empty)"}
                  </span>
                  <div className="flex gap-4">
                    <Button
                      variant="plain"
                      size="text"
                      onClick={() => setExpanded(open ? null : entry.id)}
                    >
                      {open ? "Hide" : "Details"}
                    </Button>
                    <Button variant="plain" size="text" onClick={() => copy(entry)}>
                      {copied === entry.id ? "Copied" : "Copy"}
                    </Button>
                    <Button
                      variant="plain-danger"
                      size="text"
                      onClick={() => remove(entry.id)}
                    >
                      Delete
                    </Button>
                  </div>
                </GridRow>

                {open && (
                  <RunDetail
                    entry={entry}
                    stages={
                      entry.utteranceId === null
                        ? undefined
                        : spans.get(entry.utteranceId)
                    }
                    spansFrom={spansFrom}
                    busy={retrying === entry.id}
                    note={note?.id === entry.id ? note.text : null}
                    onRetry={() => retry(entry)}
                    onRetryTranscription={(engine) =>
                      retryTranscription(entry, engine)
                    }
                  />
                )}
              </div>
            );
          })
        )}
      </Group>
    </div>
  );
}

function RunDetail({
  entry,
  stages,
  spansFrom,
  busy,
  note,
  onRetry,
  onRetryTranscription,
}: {
  entry: Entry;
  /** Undefined when the run predates `utteranceId`, or its spans were pruned. */
  stages: Span[] | undefined;
  /** Oldest utterance the spans fetch covered; null when none were read. */
  spansFrom: number | null;
  busy: boolean;
  note: string | null;
  onRetry: () => void;
  onRetryTranscription: (engine: "local" | "cloud" | null) => void;
}) {
  const cleanup = stages?.find((s) => s.stage === "cleanup");
  // Recorded by the cleanup span as a joined string. Empty means the
  // vocabulary was empty; a pre-4.4 run has no field at all, hence the null.
  const vocabulary = useMemo(() => {
    const raw = cleanup && spanFields(cleanup).vocabulary;
    if (typeof raw !== "string") return null;
    return raw.split(",").map((t) => t.trim()).filter(Boolean);
  }, [cleanup]);

  return (
    <div className="flex flex-col gap-4 pb-4">
      <div className="grid grid-cols-2 gap-4">
        <Transcript title="Heard" text={entry.rawTranscript} />
        <Transcript title="Pasted" text={entry.finalTranscript} />
      </div>

      {entry.audioPath && (
        <div>
          <Label>Recording</Label>
          {/* Tauri's asset protocol, scoped in tauri.conf.json to the media
              directory. A path outside that scope 403s, which the <audio>
              element reports as a generic media error — it looks like a
              corrupt file, not a permission problem. */}
          <audio
            controls
            preload="none"
            className="h-8 w-full max-w-md"
            src={convertFileSrc(entry.audioPath)}
          />
        </div>
      )}

      {vocabulary && (
        <div>
          <Label>Vocabulary in play</Label>
          {vocabulary.length === 0 ? (
            <p className="text-hint text-ink-2">
              None — nothing was given to the prompt as a spelling hint.
            </p>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {vocabulary.map((term) => (
                <span
                  key={term}
                  className="rounded-[7px] bg-field px-2 py-[3px] text-hint"
                >
                  {term}
                </span>
              ))}
            </div>
          )}
        </div>
      )}

      <div>
        <Label>Pipeline</Label>
        {stages === undefined || stages.length === 0 ? (
          <p className="text-hint text-ink-2">
            {spansFrom !== null && entry.utteranceId !== null && entry.utteranceId < spansFrom
              ? `No breakdown loaded — this pane reads the ${FETCH} most recent runs' spans, and this one is older than that.`
              : "No breakdown for this run — either it predates per-run timings, or its spans have since been pruned."}
          </p>
        ) : (
          <div className="flex flex-col gap-1">
            {stages
              .filter((s) => s.stage !== "utterance")
              .map((s, i) => (
                <div
                  key={`${s.stage}-${i}`}
                  className="grid grid-cols-[120px_58px_1fr] items-baseline gap-2 text-hint"
                >
                  <span className={s.success ? "" : "text-danger"}>
                    {s.stage}
                    {!s.success && " ✗"}
                  </span>
                  <span className="text-right tabular-nums text-ink-2">
                    {s.durationMs} ms
                  </span>
                  <span className="truncate font-mono text-ink-2" title={s.fields}>
                    {s.fields}
                  </span>
                </div>
              ))}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Button variant="plain" size="text" onClick={onRetry} disabled={busy}>
          {busy ? "Re-running…" : "Re-run cleanup"}
        </Button>
        {entry.audioPath && (
          <>
            <Button
              variant="plain"
              size="text"
              onClick={() => onRetryTranscription(null)}
              disabled={busy}
            >
              Re-transcribe
            </Button>
            {/* A native select: two options do not justify a popover, and this
                is keyboard and screen-reader correct for free. */}
            <select
              aria-label="Re-transcribe using a different engine"
              className="rounded-[7px] bg-field px-2 py-[3px] text-hint"
              disabled={busy}
              value=""
              onChange={(e) => {
                const engine = e.target.value;
                e.target.value = "";
                if (engine === "local" || engine === "cloud") {
                  onRetryTranscription(engine);
                }
              }}
            >
              <option value="">Other engine…</option>
              <option value="local">On-device</option>
              <option value="cloud">Cloud</option>
            </select>
          </>
        )}
        {note && <span className="text-hint text-ink-2">{note}</span>}
      </div>
    </div>
  );
}

function Label({ children }: { children: string }) {
  return (
    <p className="mb-1 text-xs font-semibold uppercase tracking-[0.03em] text-ink-2">
      {children}
    </p>
  );
}

function Transcript({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0">
      <Label>{title}</Label>
      <p className="whitespace-pre-wrap break-words text-hint">
        {text || "(empty)"}
      </p>
    </div>
  );
}
