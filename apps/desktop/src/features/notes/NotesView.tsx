import { useEffect, useMemo, useState } from "react";
import { commands, type Note, type SearchResult } from "../../lib/bindings";
import { Highlight } from "./Highlight";
import { or } from "../../lib/result";
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

  useEffect(() => {
    or(commands.recentNotes(20), [], "loading notes").then(setRecent);
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
    <div className="notes">
      <input
        className="notes__search"
        type="search"
        placeholder="Search notes"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        autoFocus
      />

      {notes.length === 0 ? (
        <p className="notes__empty">
          {showing ? `No notes match “${query}”` : "No notes yet."}
        </p>
      ) : (
        <ul className="notes__list">
          {notes.map((note, i) => (
            <li key={note.path}>
              <button className="notes__item" onClick={() => setOpen(note)}>
                <span className="notes__title">
                  <Highlight text={note.title} terms={terms} />
                </span>
                <span className="notes__snippet">
                  {showing ? (
                    <Highlight text={results[i].snippet} terms={terms} />
                  ) : (
                    formatDate(note.date)
                  )}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Reader({ note, onBack }: { note: Note; onBack: () => void }) {
  return (
    <div className="reader">
      <button className="reader__back" onClick={onBack}>
        ← Search
      </button>
      <h2 className="reader__title">{note.title}</h2>
      <time className="reader__date">{formatDate(note.date)}</time>
      {/* Rendered as text, not HTML: a note is untrusted input, and dangerously
          setting innerHTML from it would be an XSS vector. */}
      <pre className="reader__body">{note.content}</pre>
    </div>
  );
}

/** `date` is unix seconds from Rust. */
function formatDate(date: number) {
  return new Date(date * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
