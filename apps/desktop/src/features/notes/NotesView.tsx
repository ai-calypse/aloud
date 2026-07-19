import { useEffect, useMemo, useState } from "react";
import { commands, events, type Note, type SearchResult } from "../../lib/bindings";
import { Highlight } from "./Highlight";
import { or } from "../../lib/result";
import { MessageResponse } from "@/components/ai-elements/message";
import { Group } from "@/components/ui/rows";
import "./notes.css";

/**
 * Search-first notes browsing, ported from `NotesSearchView.swift`.
 *
 * Ranking lives in Rust (`notes::search`), so results here are already in the
 * right order — this only renders them.
 */
export function NotesView() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [recent, setRecent] = useState<Note[]>([]);
  const [open, setOpen] = useState<Note | null>(null);

  // Whitespace split, matching Rust's `notes::terms`, so highlighting marks
  // exactly what scored.
  const terms = useMemo(() => query.split(/\s+/).filter(Boolean), [query]);

  // Fetched on mount, then again on window focus and whenever a meeting
  // finishes saving — the list went stale when a note landed while this view
  // was already open.
  useEffect(() => {
    const refresh = () =>
      or(commands.recentNotes(20), [], "loading notes").then(setRecent);
    refresh();
    window.addEventListener("focus", refresh);
    const unlisten = events.meetingPhase.listen(({ payload }) => {
      if (payload === "idle") refresh();
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

  if (open) return <Reader note={open} onBack={() => setOpen(null)} />;

  const showing = terms.length > 0;
  const notes = showing ? results.map((r) => r.note) : recent;

  return (
    <div className="flex min-h-0 flex-col gap-3.5">
      <div className="flex items-center gap-3">
        <div className="flex flex-1 items-center gap-[7px] rounded-field border-[0.5px] border-surface-border bg-surface px-3 py-2 shadow-field focus-within:border-brand">
          <svg
            viewBox="0 0 16 16"
            className="size-[13px] shrink-0 text-search-icon"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
          >
            <circle cx="7" cy="7" r="4.2" />
            <line x1="10.2" y1="10.2" x2="13.5" y2="13.5" strokeLinecap="round" />
          </svg>
          <input
            type="search"
            className="w-full bg-transparent text-ui outline-none placeholder:text-ink-2"
            placeholder="Search notes"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
        </div>
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

function Reader({ note, onBack }: { note: Note; onBack: () => void }) {
  // Reading is the common case; the source is there for copying a section out
  // or checking what the summarizer actually wrote.
  const [source, setSource] = useState(false);

  return (
    <div className="reader">
      <div className="reader__bar">
        <button className="reader__back" onClick={onBack}>
          ← Search
        </button>
        <button
          className="reader__toggle"
          onClick={() => setSource((s) => !s)}
          aria-pressed={source}
        >
          {source ? "Preview" : "Markdown"}
        </button>
      </div>
      <h2 className="reader__title">{note.title}</h2>
      <time className="reader__date">{formatDate(note.date)}</time>

      {source ? (
        <pre className="reader__body">{note.content}</pre>
      ) : (
        /* Streamdown parses the markdown to React elements — no innerHTML, so
           a note staying untrusted input costs nothing. Same renderer the ask
           and agent panels use. */
        <MessageResponse className="reader__rendered">
          {note.content}
        </MessageResponse>
      )}
    </div>
  );
}

/** `date` is unix seconds from Rust. Middot separator, as in the design. */
function formatDate(date: number) {
  const at = new Date(date * 1000);
  return `${at.toLocaleDateString(undefined, { dateStyle: "medium" })} · ${at.toLocaleTimeString(undefined, { timeStyle: "short" })}`;
}
