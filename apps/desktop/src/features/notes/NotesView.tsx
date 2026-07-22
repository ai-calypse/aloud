import { useEffect, useMemo, useState } from "react";
import { commands, events, type Note, type SearchResult } from "../../lib/bindings";
import { Highlight } from "./Highlight";
import { or } from "../../lib/result";
import { MessageResponse } from "@/components/ai-elements/message";
import { Group } from "@/components/ui/rows";
import { SearchField } from "@/components/ui/search-field";
import { formatDate } from "@/lib/format";
import "./notes.css";

/** Search-first notes browsing. Ranking lives in Rust (`notes::search`), so
 * results arrive already ordered — this only renders them. */
export function NotesView() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [recent, setRecent] = useState<Note[]>([]);
  const [open, setOpen] = useState<Note | null>(null);

  // Whitespace split, matching Rust's `notes::terms`, so highlighting marks
  // exactly what scored.
  const terms = useMemo(() => query.split(/\s+/).filter(Boolean), [query]);

  // Refetched on focus and when a meeting finishes saving: a note can land
  // while this view is already open.
  useEffect(() => {
    const refresh = () =>
      or(commands.recentNotes(20), [], "loading notes").then(setRecent);
    refresh();
    window.addEventListener("focus", refresh);
    const unlisten = events.meetingPhase.listen(({ payload }) => {
      if (payload.phase === "idle") refresh();
    });
    return () => {
      window.removeEventListener("focus", refresh);
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    if (terms.length === 0) {
      setResults([]);
      return;
    }
    // Notes are read from disk per query; a keystroke-rate call is wasteful.
    const timer = setTimeout(() => {
      or(commands.searchNotes(query), [], "searching notes").then(setResults);
    }, 80);
    return () => clearTimeout(timer);
  }, [query, terms.length]);

  if (open) return <NoteReader note={open} onBack={() => setOpen(null)} />;

  const showing = terms.length > 0;
  const notes = showing ? results.map((r) => r.note) : recent;

  return (
    <div className="flex min-h-0 flex-col gap-3.5">
      <div className="flex items-center gap-3">
        <SearchField
          className="flex-1"
          value={query}
          onChange={setQuery}
          placeholder="Search notes"
          autoFocus
        />
        <span className="shrink-0 text-xs text-ink-2">
          {notes.length} {notes.length === 1 ? "note" : "notes"}
        </span>
      </div>

      {notes.length === 0 ? (
        <p className="text-ui text-ink-2">
          {showing ? `No notes match “${query}”` : "No notes yet."}
        </p>
      ) : (
        <Group className="min-h-0 overflow-y-auto">
          {notes.map((note, i) => (
            <button
              key={note.path}
              onClick={() => setOpen(note)}
              className="grid w-full grid-cols-[1fr_auto] items-center gap-3.5 border-b-[0.5px] border-separator py-3 text-left last:border-b-0 hover:bg-hover"
            >
              <span className="truncate text-sm font-medium">
                <Highlight text={note.title} terms={terms} />
              </span>
              <span className="shrink-0 whitespace-nowrap text-xs text-ink-2">
                {formatDate(note.date)}
              </span>
              {showing && (
                <span className="col-span-2 truncate text-xs text-ink-2">
                  <Highlight text={results[i].snippet} terms={terms} />
                </span>
              )}
            </button>
          ))}
        </Group>
      )}
    </div>
  );
}

/** Exported so the Meetings pane can read a note without a second reader. */
export function NoteReader({
  note,
  onBack,
  backLabel = "Search",
}: {
  note: Note;
  onBack: () => void;
  backLabel?: string;
}) {
  const [source, setSource] = useState(false);

  return (
    <div className="flex h-full flex-col gap-[0.4rem]">
      <div className="flex items-center justify-between gap-2">
        <button className="py-[0.3rem] text-brand" onClick={onBack}>
          ← {backLabel}
        </button>
        <button
          className="rounded-full border border-border px-[0.6rem] py-1 text-[0.78rem] text-ink-2 hover:text-inherit"
          onClick={() => setSource((s) => !s)}
          aria-pressed={source}
        >
          {source ? "Preview" : "Markdown"}
        </button>
      </div>
      <h2 className="mt-[0.4rem] text-[1.2rem]">{note.title}</h2>
      <time className="text-ui text-ink-2">{formatDate(note.date)}</time>

      {source ? (
        <pre className="mt-[0.8rem] overflow-y-auto font-mono text-[0.82rem] leading-[1.55] break-words whitespace-pre-wrap">
          {note.content}
        </pre>
      ) : (
        /* Streamdown parses markdown to React elements — no innerHTML, so a
           note stays untrusted input safely. `reader__rendered` is only a hook
           for the descendant rules in notes.css. */
        <MessageResponse className="reader__rendered mt-[0.8rem] overflow-y-auto text-[0.9rem] leading-[1.6]">
          {note.content}
        </MessageResponse>
      )}
    </div>
  );
}
