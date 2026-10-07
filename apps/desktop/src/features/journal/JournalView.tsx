import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, Copy, FolderOpen, Mic, Square } from "lucide-react";
import {
  commands,
  events,
  type Binding,
  type Note,
  type SearchResult,
} from "@/lib/bindings";
import { or } from "@/lib/result";
import { createAutosaver } from "@/lib/autosave";
import { formatDate } from "@/lib/format";
import { entryPreview, groupByDay, wordCount } from "@/lib/journal";
import { cn } from "@/lib/utils";
import { Caption, Surface } from "@/components/ui/rows";
import { Button } from "@/components/ui/button";
import { SearchField } from "@/components/ui/search-field";
import { Highlight } from "../notes/Highlight";
import { describeBinding } from "../settings/binding-label";
import "./journal.css";

/** Entries the timeline holds. Rust reads every file per call anyway, so this
 * bounds rendering, not disk: years of daily thoughts still fit. */
const LIMIT = 500;

/** The unsent composer text, kept per window so leaving the pane loses
 * nothing. A convenience, not a store: a blocked localStorage just forgets. */
const DRAFT_KEY = "aloud.journal.draft";

function loadDraft(): string {
  try {
    return localStorage.getItem(DRAFT_KEY) ?? "";
  } catch {
    return "";
  }
}

function keepDraft(text: string) {
  try {
    if (text) localStorage.setItem(DRAFT_KEY, text);
    else localStorage.removeItem(DRAFT_KEY);
  } catch {
    /* a blocked store just means the draft is not remembered */
  }
}

const timeOf = (date: number) =>
  new Date(date * 1000).toLocaleTimeString(undefined, { timeStyle: "short" });

/**
 * The journal: thoughts spoken or typed, kept as plain files on this Mac.
 * The home of the core app. Ranking, storage and the capture itself are
 * Rust's (`aloud_notes::journal`, `Pipeline::stop_and_journal`); this lists
 * and edits.
 */
export function JournalView({
  captureKey,
  onOpenHotkeys,
}: {
  /** The Capture thought binding, to say how to save from any app. */
  captureKey: Binding | null | undefined;
  onOpenHotkeys: () => void;
}) {
  const [entries, setEntries] = useState<Note[] | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [open, setOpen] = useState<Note | null>(null);
  // The entry that just arrived, which grows into the list rather than
  // appearing: a capture lands while the user is looking elsewhere.
  const [fresh, setFresh] = useState<string | null>(null);

  const refresh = useCallback(async (arrived?: string) => {
    const list = await or(commands.journalEntries(LIMIT), [], "loading the journal");
    setEntries(list);
    if (arrived) setFresh(arrived);
  }, []);

  // Reread on focus (App Nap can freeze this webview past an event) and when a
  // capture lands from the Journal key, the menu bar or the record button.
  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    const off = events.journalChanged.listen(({ payload }) => void refresh(payload.path));
    return () => {
      window.removeEventListener("focus", onFocus);
      off.then((stop) => stop());
    };
  }, [refresh]);

  // Whitespace split, matching Rust's `notes::terms`, so highlighting marks
  // exactly what scored.
  const terms = useMemo(() => query.split(/\s+/).filter(Boolean), [query]);
  useEffect(() => {
    if (terms.length === 0) {
      setResults([]);
      return;
    }
    // Every file is read per query; a keystroke-rate call is wasteful.
    const timer = setTimeout(() => {
      or(commands.journalSearch(query), [], "searching the journal").then(setResults);
    }, 80);
    return () => clearTimeout(timer);
  }, [query, terms.length]);

  if (open) {
    return (
      <EntryEditor
        entry={open}
        onClose={() => {
          setOpen(null);
          void refresh();
        }}
      />
    );
  }

  const searching = terms.length > 0;
  const days = entries ? groupByDay(entries) : [];

  return (
    <div className="flex flex-col gap-5">
      <Composer
        captureKey={captureKey}
        onOpenHotkeys={onOpenHotkeys}
        onSaved={(note) => {
          setEntries((list) => [note, ...(list ?? []).filter((e) => e.path !== note.path)]);
          setFresh(note.path);
        }}
      />

      {entries !== null && entries.length > 0 && (
        <div className="flex items-center gap-3">
          <SearchField
            className="flex-1"
            value={query}
            onChange={setQuery}
            placeholder="Search your journal"
          />
          <Button
            variant="plain"
            size="text"
            onClick={() => void commands.journalReveal()}
            title="Your entries are plain text files in this folder"
          >
            <FolderOpen className="size-3.5" aria-hidden />
            Show files
          </Button>
        </div>
      )}

      {entries === null ? null : searching ? (
        results.length === 0 ? (
          <p className="px-1 text-ui text-ink-2">Nothing in your journal matches “{query}”.</p>
        ) : (
          <Surface className="px-[15px]">
            {results.map((r) => (
              <EntryRow
                key={r.note.path}
                entry={r.note}
                when={formatDate(r.note.date)}
                preview={r.snippet}
                terms={terms}
                onOpen={() => setOpen(r.note)}
              />
            ))}
          </Surface>
        )
      ) : days.length === 0 ? (
        <Surface className="px-6 py-10 text-center">
          <p className="mb-1 text-[15px] font-semibold text-foreground">Nothing here yet</p>
          <p className="text-ui text-ink-2">
            Speak or type a thought above. Entries are saved on this Mac as plain
            text files you can open anywhere.
          </p>
        </Surface>
      ) : (
        days.map((day) => (
          <section key={day.key}>
            <Caption>{day.label}</Caption>
            <Surface className="px-[15px]">
              {day.entries.map((entry) => (
                <EntryRow
                  key={entry.path}
                  entry={entry}
                  when={timeOf(entry.date)}
                  preview={entryPreview(entry.content)}
                  arriving={entry.path === fresh}
                  onArrived={() => setFresh(null)}
                  onOpen={() => setOpen(entry)}
                />
              ))}
            </Surface>
          </section>
        ))
      )}
    </div>
  );
}

/** One entry in a list: when, what it starts with, and a copy action. */
function EntryRow({
  entry,
  when,
  preview,
  terms = [],
  arriving = false,
  onArrived,
  onOpen,
}: {
  entry: Note;
  when: string;
  preview: string;
  terms?: string[];
  arriving?: boolean;
  onArrived?: () => void;
  onOpen: () => void;
}) {
  return (
    <div
      className="motion-row [&:last-child>*]:border-b-0"
      data-motion={arriving ? "in" : undefined}
      // `animationend` bubbles; only the row's own entrance settles it.
      onAnimationEnd={(e) => {
        if (e.target === e.currentTarget) onArrived?.();
      }}
    >
      <div className="border-b-[0.5px] border-separator">
        <div className="group grid grid-cols-[auto_1fr_auto] items-start gap-3.5 py-3">
          <time className="min-w-[64px] pt-px text-xs tabular-nums text-ink-2">{when}</time>
          <button onClick={onOpen} className="min-w-0 text-left">
            <p className="truncate text-sm font-medium">
              <Highlight text={entry.title || "Untitled"} terms={terms} />
            </p>
            {preview && (
              <p className="mt-0.5 line-clamp-2 text-xs leading-relaxed text-ink-2">
                <Highlight text={preview} terms={terms} />
              </p>
            )}
          </button>
          <CopyButton
            text={entry.content}
            className="opacity-0 transition-opacity duration-[var(--dur-fast)] group-focus-within:opacity-100 group-hover:opacity-100"
          />
        </div>
      </div>
    </div>
  );
}

/** Copies `text`, and says so for a moment. */
function CopyButton({
  text,
  labelled = false,
  className,
}: {
  text: string;
  labelled?: boolean;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      variant="plain"
      size="text"
      className={cn(copied && "opacity-100", className)}
      aria-label="Copy entry"
      onClick={() =>
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
        })
      }
    >
      {copied ? (
        <Check className="size-3.5" aria-hidden />
      ) : (
        <Copy className="size-3.5" aria-hidden />
      )}
      {labelled && (copied ? "Copied" : "Copy")}
    </Button>
  );
}

/** Where the record button is: what a click does next. */
type CaptureState = "idle" | "recording" | "saving";

/**
 * The record button's state, kept in step with Rust. A capture can also be
 * started or stopped from the Journal key or the menu bar, and Escape cancels
 * one, so the phase events are the truth and the click only predicts them.
 */
function useCapture() {
  const [state, setState] = useState<CaptureState>("idle");
  const [note, setNote] = useState<string | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    // A window opened mid-capture shows it rather than a stale button.
    void commands.journalCapturing().then((on) => on && setState("recording"));
    const off = events.dictationPhase.listen(({ payload }) => {
      if (stateRef.current === "idle") return;
      if (payload === "transcribing") setState("saving");
      if (payload === "saved" || payload === "idle") setState("idle");
      if (payload === "noSpeech") {
        setState("idle");
        setNote("Nothing was heard, so nothing was saved.");
      }
      if (payload === "error") {
        setState("idle");
        setNote("That thought could not be saved. The pill at the top says why.");
      }
    });
    return () => {
      off.then((stop) => stop());
    };
  }, []);

  const toggle = useCallback(async () => {
    setNote(null);
    const result = await commands.journalCapture();
    if (result.status === "error") {
      setNote(result.error);
      return;
    }
    setState(result.data ? "recording" : "saving");
  }, []);

  return { state, note, toggle };
}

/** Typing or speaking a new entry. */
function Composer({
  captureKey,
  onOpenHotkeys,
  onSaved,
}: {
  captureKey: Binding | null | undefined;
  onOpenHotkeys: () => void;
  onSaved: (entry: Note) => void;
}) {
  const [text, setText] = useState(loadDraft);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const capture = useCapture();
  const words = wordCount(text);

  async function save() {
    if (!text.trim() || saving) return;
    setSaving(true);
    const result = await commands.journalSave(text, null);
    setSaving(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setError(null);
    setText("");
    keepDraft("");
    onSaved(result.data);
  }

  const keyLabel =
    captureKey && captureKey.kind !== "disabled" ? describeBinding(captureKey) : null;
  const status =
    capture.state === "recording"
      ? "Listening — click to stop and save"
      : capture.state === "saving"
        ? "Writing it down…"
        : null;

  return (
    <Surface className="flex flex-col gap-3 p-4">
      <textarea
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          keepDraft(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void save();
          }
        }}
        placeholder="What’s on your mind?"
        aria-label="New journal entry"
        className="field-sizing-content max-h-[40vh] min-h-[72px] w-full resize-none bg-transparent text-[15px] leading-relaxed outline-none placeholder:text-ink-3"
      />
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => void capture.toggle()}
          disabled={capture.state === "saving"}
          data-state={capture.state}
          aria-label={capture.state === "recording" ? "Stop and save" : "Speak a thought"}
          className={cn(
            "journal-rec inline-flex h-8 items-center gap-1.5 rounded-full px-3 text-ui font-medium",
            "transition-[background-color,color,transform] duration-[var(--dur-fast)] active:scale-[0.97]",
            capture.state === "idle" && "bg-field text-ink hover:bg-brand hover:text-brand-fg",
            capture.state === "saving" && "bg-field text-ink-2",
          )}
        >
          {capture.state === "recording" ? (
            <Square className="size-3 fill-current" aria-hidden />
          ) : (
            <Mic className="size-3.5" aria-hidden />
          )}
          {capture.state === "recording" ? "Stop" : "Speak"}
        </button>
        <span className="min-w-0 truncate text-hint text-ink-2" role="status">
          {status ?? capture.note ?? (words > 0 ? `${words} ${words === 1 ? "word" : "words"} · ⌘↩ to save` : "")}
        </span>
        <Button
          variant="accent"
          size="row"
          className="ml-auto"
          disabled={!text.trim() || saving}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
      {error && <p className="text-hint text-warning">{error}</p>}
      <p className="border-t-[0.5px] border-separator pt-3 text-hint text-ink-2">
        {keyLabel ? (
          <>
            Hold <span className="font-medium text-ink">{keyLabel}</span> in any app to
            capture a thought without leaving it.
          </>
        ) : (
          <>
            Capture a thought from any app with a key of your own —{" "}
            <button className="text-brand hover:text-brand/80" onClick={onOpenHotkeys}>
              set one in Hotkeys
            </button>
            , or use Capture a Thought in the menu bar.
          </>
        )}
      </p>
    </Surface>
  );
}

/** Reading and editing one entry. Saves as you type; nothing to forget. */
function EntryEditor({ entry, onClose }: { entry: Note; onClose: () => void }) {
  const [text, setText] = useState(entry.content);
  const [status, setStatus] = useState<"saved" | "saving" | "failed" | "empty">("saved");
  const [confirming, setConfirming] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const saver = useMemo(
    () =>
      createAutosaver(async (next) => {
        // The crate refuses an empty entry; deleting is the way to remove one.
        if (!next.trim()) {
          setStatus("empty");
          return;
        }
        setStatus("saving");
        const result = await commands.journalSave(next, entry.path);
        if (result.status === "error") {
          setStatus("failed");
          throw new Error(result.error);
        }
        setStatus("saved");
      }),
    [entry.path],
  );

  // Leaving by any route — the back button, another pane — keeps the last
  // keystrokes.
  useEffect(() => () => void saver.flush(), [saver]);

  async function close() {
    await saver.flush();
    onClose();
  }

  async function remove() {
    const result = await commands.journalDelete(entry.path);
    if (result.status === "error") {
      setDeleteError(result.error);
      return;
    }
    onClose();
  }

  const statusText = {
    saved: "Saved",
    saving: "Saving…",
    failed: "Not saved — keep typing to retry",
    empty: "An empty entry isn’t saved",
  }[status];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <button className="py-[0.3rem] text-brand" onClick={() => void close()}>
          ← Journal
        </button>
        <span className={cn("text-hint", status === "failed" ? "text-warning" : "text-ink-2")}>
          {statusText}
        </span>
      </div>
      <time className="text-ui text-ink-2">{formatDate(entry.date)}</time>
      <Surface className="p-4">
        <textarea
          autoFocus
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            saver.schedule(e.target.value);
          }}
          aria-label="Journal entry"
          className="field-sizing-content min-h-[45vh] w-full resize-none bg-transparent text-[15px] leading-relaxed outline-none"
        />
      </Surface>
      <div className="flex items-center gap-4">
        <CopyButton text={text} labelled />
        <span className="text-hint text-ink-2">{wordCount(text)} words</span>
        <div className="ml-auto flex items-center gap-3">
          {confirming ? (
            <>
              <span className="text-hint text-ink-2">Delete this entry for good?</span>
              <Button variant="plain-danger" size="text" onClick={() => void remove()}>
                Delete
              </Button>
              <Button variant="plain" size="text" onClick={() => setConfirming(false)}>
                Keep
              </Button>
            </>
          ) : (
            <Button variant="plain-danger" size="text" onClick={() => setConfirming(true)}>
              Delete…
            </Button>
          )}
        </div>
      </div>
      {deleteError && <p className="text-hint text-warning">{deleteError}</p>}
    </div>
  );
}
