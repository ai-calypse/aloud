import { useEffect, useRef, useState } from "react";
import { commands, type Note } from "@/lib/bindings";
import { createAutosaver } from "@/lib/autosave";

/**
 * The quick-notes behaviour of Focus › Notes, for any look to draw: type and
 * it is saved (a markdown file in the notes folder) after every pause and when
 * you leave; open an older one; start a new one. The same rules as the calm
 * view, kept in one place here so the widgets cannot drift from them.
 */
export function useQuickNotes() {
  const [text, setText] = useState("");
  const [notes, setNotes] = useState<Note[]>([]);
  const path = useRef<string | null>(null);

  const reload = () =>
    commands
      .quickNotes()
      .then((r) => r.status === "ok" && setNotes(r.data.filter((n) => n.content.trim() !== "")))
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

  return { text, notes, openPath: path.current, edit, open, fresh };
}
