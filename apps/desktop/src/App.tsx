import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands, type Settings, type Stats } from "./lib/bindings";
import { NotesView } from "./features/notes/NotesView";
import "./App.css";

/** Mirrors the Rust `pipeline::Phase`. */
type Phase = "idle" | "recording" | "transcribing" | "done" | "error";

const PHASE_LABEL: Record<Phase, string> = {
  idle: "Ready",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  error: "Something went wrong",
};

type Tab = "notes" | "status";

export default function App() {
  const [tab, setTab] = useState<Tab>("notes");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [hotkeysReady, setHotkeysReady] = useState(true);

  useEffect(() => {
    commands.getSettings().then(setSettings);
    commands.getStats().then(setStats);
    commands.hotkeysPermitted().then(setHotkeysReady);

    const unlisten = listen<Phase>("dictation-phase", (event) => {
      setPhase(event.payload);
      // Stats only change once the text has actually landed.
      if (event.payload === "done") commands.getStats().then(setStats);
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  if (!settings) return <main className="app">Loading…</main>;

  return (
    <main className="app">
      <nav className="tabs">
        {(["notes", "status"] as const).map((t) => (
          <button
            key={t}
            className={`tab ${tab === t ? "tab--active" : ""}`}
            onClick={() => setTab(t)}
          >
            {t === "notes" ? "Notes" : "Status"}
          </button>
        ))}
      </nav>

      {!hotkeysReady && (
        <p className="warning">
          Accessibility permission is needed for hotkeys. Grant it in System
          Settings → Privacy &amp; Security → Accessibility, then restart.
        </p>
      )}

      {tab === "notes" ? (
        <NotesView />
      ) : (
        <section>
          <p className={`phase phase--${phase}`}>{PHASE_LABEL[phase]}</p>

          <dl className="stats">
            <dt>Words</dt>
            <dd>{stats?.totalWords ?? 0}</dd>
            <dt>Sessions</dt>
            <dd>{stats?.totalSessions ?? 0}</dd>
            <dt>Streak</dt>
            <dd>{stats?.currentStreakDays ?? 0} days</dd>
          </dl>

          <p className="hint">
            Engine: {settings.engine === "local" ? "on-device whisper" : "cloud"}
          </p>
        </section>
      )}
    </main>
  );
}
