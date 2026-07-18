import { useEffect, useState } from "react";
import {
  commands,
  events,
  type Phase,
  type Settings,
  type Stats,
} from "./lib/bindings";
import { NotesView } from "./features/notes/NotesView";
import {
  SettingsView,
  type SettingsSection,
} from "./features/settings/SettingsView";
import { Onboarding } from "./features/onboarding/Onboarding";
import { cn } from "@/lib/utils";
import "./App.css";

const PHASE_LABEL: Record<Phase, string> = {
  idle: "Ready",
  recording: "Listening…",
  transcribing: "Transcribing…",
  done: "Pasted",
  error: "Something went wrong",
};

/** A sidebar row: either a top-level view or one settings pane. */
type Nav = "notes" | "status" | SettingsSection;

// icon is an inline SVG path — no icon dependency. Grouping mirrors the Swift
// app: content views on top, settings panes below a divider.
const NAV: { id: Nav; title: string; icon: string; group: "app" | "settings" }[] =
  [
    { id: "notes", title: "Notes", group: "app", icon: "M4 4a2 2 0 0 1 2-2h8l6 6v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4ZM14 2v6h6M8 13h8M8 17h8" },
    { id: "status", title: "Status", group: "app", icon: "M3 12h4l3 8 4-16 3 8h4" },
    { id: "general", title: "General", group: "settings", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-2.7-1.1l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0-1.1-2.7H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.1-2.7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 2.7-1.1V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 2.7 1.1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0 1.1 2.7H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1Z" },
    { id: "hotkeys", title: "Hotkeys", group: "settings", icon: "M4 6h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1ZM7 10h.01M11 10h.01M15 10h.01M8 14h8" },
    { id: "macros", title: "Voice Macros", group: "settings", icon: "M12 2a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3ZM5 11a7 7 0 0 0 14 0M12 18v4" },
    { id: "rules", title: "App Rules", group: "settings", icon: "M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V5ZM4 9h16M8 6h.01" },
  ];

const SETTINGS_IDS: SettingsSection[] = ["general", "hotkeys", "macros", "rules"];

const isSettings = (n: Nav): n is SettingsSection =>
  (SETTINGS_IDS as string[]).includes(n);

export default function App() {
  const [nav, setNav] = useState<Nav>("notes");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [stats, setStats] = useState<Stats | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [hotkeysReady, setHotkeysReady] = useState(true);

  useEffect(() => {
    commands.getSettings().then(setSettings);
    commands.getStats().then(setStats);
    commands.hotkeysPermitted().then(setHotkeysReady);

    const unlistenPhase = events.dictationPhase.listen((event) => {
      setPhase(event.payload);
      // Stats only change once the text has actually landed.
      if (event.payload === "done") commands.getStats().then(setStats);
    });
    // Rust waits for the Accessibility grant and announces it, so the banner
    // clears the moment permission lands — no app restart needed.
    const unlistenPermission = events.hotkeysPermitted.listen((event) =>
      setHotkeysReady(event.payload),
    );
    return () => {
      unlistenPhase.then((off) => off());
      unlistenPermission.then((off) => off());
    };
  }, []);

  if (!settings)
    return <main className="p-8 text-foreground">Loading…</main>;

  // First run: the wizard replaces the shell until it's completed.
  if (!settings.onboarded) {
    return (
      <main className="h-screen overflow-y-auto p-8 text-foreground">
        <Onboarding settings={settings} onDone={setSettings} />
      </main>
    );
  }

  const active = NAV.find((n) => n.id === nav)!;

  return (
    <main className="grid h-screen grid-cols-[200px_1fr] box-border">
      {/* Glass sidebar sits directly on the native vibrancy (transparent). Top
          padding clears the macOS traffic lights (Overlay title-bar style). */}
      <nav
        className="flex flex-col gap-0.5 overflow-y-auto px-2.5 pb-3 pt-11"
        data-tauri-drag-region
      >
        {NAV.map((n, i) => (
          <div key={n.id} className="contents">
            {n.group === "settings" && NAV[i - 1]?.group === "app" && (
              <div className="my-2 mx-2 h-px bg-border" />
            )}
            <button
              onClick={() => setNav(n.id)}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-lg border border-transparent px-2.5 py-[7px] text-left text-[0.9rem] transition-colors",
                nav === n.id
                  ? "border-primary/35 bg-primary/20 font-semibold"
                  : "hover:bg-muted",
              )}
            >
              <svg
                className={cn(
                  "size-[17px] shrink-0",
                  nav === n.id ? "text-primary" : "text-muted-foreground",
                )}
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.7"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d={n.icon} />
              </svg>
              <span>{n.title}</span>
            </button>
          </div>
        ))}
      </nav>

      {/* Scrim over the vibrancy keeps the reading pane high-contrast. */}
      <div className="flex flex-col gap-5 overflow-y-auto border-l border-border bg-background p-8">
        <h1 className="text-2xl font-bold">{active.title}</h1>

        {!hotkeysReady && (
          <p className="rounded-lg bg-[var(--warning-bg)] px-4 py-3 text-sm leading-relaxed text-[var(--warning)]">
            Accessibility permission is needed for hotkeys. Grant it in System
            Settings → Privacy &amp; Security → Accessibility, then restart.
          </p>
        )}

        {nav === "notes" && <NotesView />}
        {isSettings(nav) && <SettingsView section={nav} />}
        {nav === "status" && (
          <section className="rounded-2xl border border-border bg-card p-5 backdrop-blur-2xl">
            <p
              className={cn(
                "text-lg",
                (phase === "recording" || phase === "transcribing") &&
                  "text-primary",
                phase === "error" && "text-[var(--warning)]",
                phase !== "recording" &&
                  phase !== "transcribing" &&
                  phase !== "error" &&
                  "text-muted-foreground",
              )}
            >
              {PHASE_LABEL[phase]}
            </p>

            <dl className="my-8 grid grid-cols-[auto_1fr] gap-x-6 gap-y-2">
              <dt className="text-sm text-muted-foreground">Words</dt>
              <dd className="m-0 tabular-nums">{stats?.totalWords ?? 0}</dd>
              <dt className="text-sm text-muted-foreground">Sessions</dt>
              <dd className="m-0 tabular-nums">{stats?.totalSessions ?? 0}</dd>
              <dt className="text-sm text-muted-foreground">Streak</dt>
              <dd className="m-0 tabular-nums">
                {stats?.currentStreakDays ?? 0} days
              </dd>
            </dl>

            <p className="text-sm text-muted-foreground">
              Engine:{" "}
              {settings.engine === "cloud" ? "cloud" : "on-device whisper"}
            </p>
          </section>
        )}
      </div>
    </main>
  );
}
