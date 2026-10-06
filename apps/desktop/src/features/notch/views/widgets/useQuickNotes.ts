import { useEffect, useRef, useState } from "react";
import { commands, type Note, type SyncState } from "@/lib/bindings";
import { syncStatus } from "./focus-widgets";
import { createAutosaver } from "@/lib/autosave";

/**
 * The quick-notes behaviour of Focus › Notes, for any look to draw: type and
 * it is saved (a markdown file in the notes folder) after every pause and when
 * you leave; open an older one; start a new one. The same rules as the calm
 * view, kept in one place here so the widgets cannot drift from them. Also
 * the copy of those notes into the Apple Notes app, which is one-way and
 * always asked for.
 */
export function useQuickNotes() {
  const [text, setText] = useState("");
  const [notes, setNotes] = useState<Note[]>([]);
  const path = useRef<string | null>(null);
  // Where each note stands against its copy in the Apple Notes app, by path.
  const [marks, setMarks] = useState<Record<string, SyncState>>({});
  const [status, setStatus] = useState<ReturnType<typeof syncStatus> | null>(null);
  const [syncing, setSyncing] = useState(false);

  const reload = () =>
    Promise.all([commands.quickNotes(), commands.noteSyncMarks()])
      .then(([n, m]) => {
        if (n.status === "ok") setNotes(n.data.filter((x) => x.content.trim() !== ""));
        if (m.status === "ok") setMarks(Object.fromEntries(m.data.map((x) => [x.path, x.state])));
      })
      .catch(() => {});

  const [saver] = useState(() =>
    createAutosaver(async (body) => {
      // A blank note that was never saved is not worth a file.
      if (path.current === null && body.trim() === "") return;
      const r = await commands.saveQuickNote(path.current, body);
      if (r.status === "ok") {
        path.current = r.data;
        void reload();
      }
    }),
  );

  useEffect(() => {
    void reload();
    // Leaving the tab or closing the panel inside the debounce must not lose
    // the last words typed.
    return () => void saver.flush();
  }, [saver]);

  const edit = (body: string) => {
    setText(body);
    setStatus(null);
    saver.schedule(body);
  };

  const open = async (note: Note) => {
    await saver.flush();
    path.current = note.path;
    setText(note.content);
  };

  const fresh = async () => {
    await saver.flush();
    path.current = null;
    setText("");
  };

  /** Copies `note` (or every pending note) into Apple Notes; the one in the editor is saved first. */
  const sync = async (note?: Note) => {
    await saver.flush();
    setSyncing(true);
    setStatus(null);
    try {
      const r = await commands.syncQuickNotes(note?.path ?? null);
      setStatus(
        r.status === "ok"
          ? syncStatus(r.data)
          : { text: r.error, permission: false, ok: false },
      );
    } catch (e) {
      setStatus({ text: String(e), permission: false, ok: false });
    }
    setSyncing(false);
    await reload();
  };

  const showInNotes = async (note: Note) => {
    const r = await commands.openNoteInNotes(note.path);
    if (r.status === "error") setStatus({ text: r.error, permission: false, ok: false });
  };

  return { text, notes, marks, status, syncing, sync, showInNotes, openPath: path.current, edit, open, fresh };
}
