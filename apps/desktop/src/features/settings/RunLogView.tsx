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
 * Every dictation that survived retention, newest first — the primary surface
 * for "what did it actually hear?". The tray's History submenu stays for quick
 * copy; this is where a run is inspected, retried and thrown away.
 *
 * Expanding joins the row to its `timings` spans on `utteranceId` and shows the
 * pipeline that produced it: each stage's duration, whether it succeeded, and
 * the fields it recorded — including the vocabulary the cleanup prompt was
 * given, which is usually the answer to a mangled proper noun.
 */
export function RunLogView() {
  const [entries, setEntries] = useState<Entry[]>([]);
  // Every span of the recent utterances, indexed by run. Fetched whole rather
  // than per row: `recent_timings` has no by-id form, and one call covers
  // every row the pane can expand.
  const [spans, setSpans] = useState<Map<number, Span[]>>(new Map());
  /** The oldest utterance the spans fetch reached. Rows older than this were
      not loaded, which is a different thing from having been pruned — and the
      two need different words, because one of them is not the user's data
      going missing. */
  const [spansFrom, setSpansFrom] = useState<number | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);
  const [retrying, setRetrying] = useState<number | null>(null);
  /** Per-row message from a retry — an error, or what changed. Never a toast:
      a rate-limit that vanishes before it is read is the one to keep. */
  const [note, setNote] = useState<{ id: number; text: string } | null>(null);

  const refresh = useCallback(() => {
    // An empty log is the normal first-run state, so a read failure logs and
    // shows the empty state rather than taking the pane down.
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
    // emitted — hence the delay. Same reasoning as the Timings pane.
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
   * Re-runs cleanup on the stored raw transcript. Deliberately `testPrompt`
   * rather than a second cleanup command: "test this prompt against this text"
   * and "clean this text again" are the same call, and a parallel path would
   * be the one that quietly stops matching the real pipeline.
   */
  async function retry(entry: Entry) {
    setRetrying(entry.id);
    setNote(null);
    // The live prompt, read now rather than at mount: the point of a retry is
    // usually that the prompt was just edited.
    const settings = await commands.getSettings();
    const run = await commands.testPrompt(
      settings.customSystemPrompt ?? "",
      entry.rawTranscript,
    );
    setRetrying(null);

    if (run.status === "error") {
      // Covers the MIN_WORDS trap: cleanup short-circuits on a short
      // transcript, so a retry there returns the same words and reads as a
      // button that does nothing. The command says so in words.
      setNote({ id: entry.id, text: run.error });
      return;
    }

    // An empty result means the prompt asked for a blank paste — the EMPTY
    // sentinel doing its job. Writing that back would replace the user's
    // transcript with nothing, and there is no undo.
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
   * Re-transcribes the stored recording, then cleans it up — the whole
   * pipeline from the WAV down, unlike `retry` above which starts from the
   * words already in the row.
   *
   * A null engine means "whatever Settings says". Switching engines here makes
   * the pipeline load the other model, which on a machine that has never run
   * on-device is a 150MB download: the button stays busy for a while and that
   * is not a hang, hence the wording below.
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
    // The command rewrote both transcripts, so the row is refetched rather
    // than patched: `rawTranscript` changed too, and a half-updated row would
    // show the old "Heard" beside the new "Pasted".
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
  // Recorded by the cleanup span as a joined string, because that is what the
  // prompt was actually handed. Empty means the vocabulary was empty, which is
  // different from "not recorded" — a pre-4.4 run has no field at all.
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
          {/* Served over Tauri's asset protocol, which is scoped in
              tauri.conf.json to the media directory and nothing else. Note the
              failure mode: an out-of-scope path is refused by the protocol
              handler with a 403 the element reports as a generic media error,
              so a widened path that stops matching the scope looks like a
              corrupt file rather than a permission problem. */}
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
            {/* A native select rather than the app's Select: two options in a
                details row do not justify a popover, and this one is keyboard
                and screen-reader correct for free. */}
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
