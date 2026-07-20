import { useEffect, useState } from "react";
import { commands, type Entry } from "../../lib/bindings";
import { or } from "../../lib/result";
import { formatDate, formatDuration } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { GridRow, Group } from "@/components/ui/rows";

/** Bounded because the pane renders every row it fetches, unvirtualised. */
const FETCH = 200;

const COLUMNS = "150px 96px 52px 1fr auto";

/**
 * Every dictation that survived retention, newest first — the primary surface
 * for "what did it actually hear?". The tray's History submenu stays for quick
 * copy; this is where a run is inspected and thrown away.
 *
 * Expanding shows `raw_transcript` beside `final_transcript`: the LLM cleanup's
 * before and after, which the row has always stored. Audio playback and the
 * per-stage breakdown belong to later tasks and are deliberately absent.
 */
export function RunLogView() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [copied, setCopied] = useState<number | null>(null);

  useEffect(() => {
    // An empty log is the normal first-run state, so a read failure logs and
    // shows the empty state rather than taking the pane down.
    or(commands.historyEntries(FETCH), [], "reading the run log").then(
      setEntries,
    );
  }, []);

  async function remove(id: number) {
    const result = await commands.deleteHistoryEntry(id);
    if (result.status === "error") {
      console.error(`deleting run ${id}: ${result.error}`);
      return;
    }
    setEntries((rows) => rows.filter((r) => r.id !== id));
  }

  async function clearAll() {
    const result = await commands.clearHistory();
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

  return (
    <div className="flex flex-col gap-6">
      <div>
        <div className="flex items-center justify-between gap-4">
          <h1 className="text-2xl font-bold leading-[1.1] tracking-[-0.02em]">
            Run Log
          </h1>
          {entries.length > 0 && (
            <Button
              variant="plain-danger"
              size="row"
              className="shrink-0 whitespace-nowrap"
              onClick={clearAll}
            >
              Clear all
            </Button>
          )}
        </div>
        <p className="mt-1.5 max-w-[520px] text-ui text-ink-2">
          Every dictation kept by the History setting. Expand a run to compare
          what was heard with what was pasted.
        </p>
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
            const changed = entry.rawTranscript !== entry.finalTranscript;
            const open = expanded === entry.id;
            return (
              <div
                key={entry.id}
                className="border-b-[0.5px] border-separator last:border-b-0"
              >
                <GridRow columns={COLUMNS} className="border-b-0 py-[13px]">
                  <span className="truncate text-hint text-ink-2 tabular-nums">
                    {formatDate(entry.createdAt)}
                  </span>
                  <span className="truncate text-hint text-ink-2">
                    {entry.appName ?? "—"}
                  </span>
                  <span className="text-hint text-ink-2 tabular-nums">
                    {formatDuration(entry.durationMs)}
                  </span>
                  <span className="truncate text-row">
                    {entry.finalTranscript || "(empty)"}
                  </span>
                  <div className="flex gap-4">
                    {changed && (
                      <Button
                        variant="plain"
                        size="text"
                        onClick={() => setExpanded(open ? null : entry.id)}
                      >
                        {open ? "Hide" : "Compare"}
                      </Button>
                    )}
                    <Button
                      variant="plain"
                      size="text"
                      onClick={() => copy(entry)}
                    >
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
                  <div className="grid grid-cols-2 gap-4 pb-4">
                    <Transcript title="Heard" text={entry.rawTranscript} />
                    <Transcript title="Pasted" text={entry.finalTranscript} />
                  </div>
                )}
              </div>
            );
          })
        )}
      </Group>
    </div>
  );
}

function Transcript({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0">
      <p className="mb-1 text-xs font-semibold uppercase tracking-[0.03em] text-ink-2">
        {title}
      </p>
      <p className="whitespace-pre-wrap break-words text-hint">
        {text || "(empty)"}
      </p>
    </div>
  );
}
