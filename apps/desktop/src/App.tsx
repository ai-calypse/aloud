import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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

// icon is an inline SVG path — no icon dependency. `tint` is the System
// Settings-style rounded icon chip behind it; each pane keeps one hue so a row
// is recognisable by colour before the label is read. Grouping mirrors the
// Swift app: content views on top, settings panes below a divider.
const NAV: {
  id: Nav;
  title: string;
  icon: string;
  tint: string;
  group: "app" | "settings";
}[] = [
  { id: "notes", title: "Notes", group: "app", tint: "#ff9f0a", icon: "M4 4a2 2 0 0 1 2-2h8l6 6v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4ZM14 2v6h6M8 13h8M8 17h8" },
  { id: "status", title: "Status", group: "app", tint: "#34c759", icon: "M3 12h4l3 8 4-16 3 8h4" },
  { id: "general", title: "General", group: "settings", tint: "#8e8e93", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-2.7-1.1l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0-1.1-2.7H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.1-2.7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 2.7-1.1V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 2.7 1.1l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0 1.1 2.7H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1Z" },
  { id: "hotkeys", title: "Hotkeys", group: "settings", tint: "#5856d6", icon: "M4 6h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1ZM7 10h.01M11 10h.01M15 10h.01M8 14h8" },
  { id: "macros", title: "Voice Macros", group: "settings", tint: "#34c759", icon: "M12 2a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3ZM5 11a7 7 0 0 0 14 0M12 18v4" },
  { id: "rules", title: "App Rules", group: "settings", tint: "#007aff", icon: "M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V5ZM4 9h16M8 6h.01" },
  { id: "prompts", title: "Prompts", group: "settings", tint: "#ff375f", icon: "M4 4h16v12H7l-3 3V4ZM8 9h8M8 12h5" },
  { id: "connectors", title: "Connectors", group: "settings", tint: "#64d2ff", icon: "M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1" },
];

const SETTINGS_IDS: SettingsSection[] = [
  "general",
  "hotkeys",
  "macros",
  "rules",
  "prompts",
  "connectors",
];

/**
 * Mirrors `Stats::time_saved_seconds` (crates/store/src/stats.rs): typing at
 * 40 wpm minus time actually spent speaking, clamped at zero. Formatted as in
 * voice-dict's MenuBarView: minutes under an hour, tenths of hours above.
 */
function timeSavedText(stats: Stats): string {
  const seconds = Math.max(
    ((stats.totalWords ?? 0) / 40) * 60 - (stats.totalSpokenSeconds ?? 0),
    0,
  );
  const minutes = seconds / 60;
  if (minutes < 60) return `~${Math.round(minutes)} min`;
  return `~${(minutes / 60).toFixed(1)} h`;
}

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

    // Dictation happens while this window is unfocused, and macOS suspends a
    // hidden webview (App Nap) — "done" events can land on a frozen runtime
    // and vanish, and the streak can roll at midnight with no event at all.
    // Refetching on focus catches both.
    const refreshStats = () => commands.getStats().then(setStats);
    window.addEventListener("focus", refreshStats);

    const unlistenPhase = events.dictationPhase.listen((event) => {
      setPhase(event.payload);
      // Stats only change once the text has actually landed.
      if (event.payload === "done") refreshStats();
    });
    // The tray's "Settings…" item opens the window on the settings pane; the
    // menu cannot navigate the webview itself.
    const unlistenSettings = listen("open-settings", () => setNav("general"));
    // "Re-run Setup…" clears `onboarded` in Rust; refetch so the wizard shows.
    const unlistenSetup = listen("rerun-setup", () =>
      commands.getSettings().then(setSettings),
    );

    // Rust waits for the Accessibility grant and announces it, so the banner
    // clears the moment permission lands — no app restart needed.
    const unlistenPermission = events.hotkeysPermitted.listen((event) =>
      setHotkeysReady(event.payload),
    );
    return () => {
      window.removeEventListener("focus", refreshStats);
      unlistenPhase.then((off) => off());
      unlistenPermission.then((off) => off());
      unlistenSettings.then((off) => off());
      unlistenSetup.then((off) => off());
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
    <main className="grid h-screen grid-cols-[214px_1fr] box-border">
      {/* Glass sidebar sits directly on the native vibrancy (transparent). Top
          padding clears the macOS traffic lights (Overlay title-bar style). */}
      <nav
        className="flex flex-col gap-1 overflow-y-auto px-2.5 pb-3 pt-11"
        data-tauri-drag-region
      >
        {NAV.map((n, i) => (
          <div key={n.id} className="contents">
            {n.group === "settings" && NAV[i - 1]?.group === "app" && (
              <div className="my-2 mx-2 h-px bg-[var(--sep)]" />
            )}
            <button
              onClick={() => setNav(n.id)}
              className={cn(
                "flex h-[30px] w-full items-center gap-2.5 rounded-[7px] px-2 text-left text-[13px] font-medium transition-colors",
                nav === n.id
                  ? "bg-[var(--accent)] font-semibold text-white shadow-[0_1px_2px_rgba(0,60,160,.35)]"
                  : "hover:bg-[var(--field)]",
              )}
            >
              <span
                className="grid size-5 shrink-0 place-items-center rounded-[5px]"
                style={{ background: n.tint }}
              >
                <svg
                  className="size-3.5 text-white"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d={n.icon} />
                </svg>
              </span>
              <span className="truncate">{n.title}</span>
            </button>
          </div>
        ))}

        <div className="mt-auto flex items-center gap-[7px] px-1.5 pt-3 text-[11px] leading-[1.6] text-[var(--text2)]">
          <span
            className={cn(
              "size-[7px] shrink-0 rounded-full",
              phase === "idle" ? "bg-[#34c759]" : "bg-[var(--accent)]",
            )}
          />
          <span>
            {PHASE_LABEL[phase]} ·{" "}
            {settings.engine === "cloud" ? "Cloud" : "On-device"}
            <br />
            {stats?.totalWords ?? 0} words
          </span>
        </div>
      </nav>

      {/* Solid canvas: the grouped setting cards need an opaque backdrop for
          their hairline separators to read. */}
      <div className="flex flex-col gap-5 overflow-y-auto border-l border-[var(--sep)] bg-[var(--bg)] px-[30px] pb-9 pt-7">
        {!isSettings(nav) && (
          <h1 className="text-2xl font-bold leading-[1.1] tracking-[-0.02em]">
            {active.title}
          </h1>
        )}

        {!hotkeysReady && (
          <p className="rounded-lg bg-[var(--warning-bg)] px-4 py-3 text-sm leading-relaxed text-[var(--warning)]">
            Accessibility permission is needed for hotkeys. Grant it in System
            Settings → Privacy &amp; Security → Accessibility, then restart.
          </p>
        )}

        {nav === "notes" && <NotesView />}
        {isSettings(nav) && <SettingsView section={nav} />}
        {nav === "status" && (
          <section className="rounded-[11px] border-[0.5px] border-[var(--card-border)] bg-[var(--card-solid)] p-5 shadow-[0_1px_2px_rgba(0,0,0,.05)]">
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
              <dt className="text-sm text-muted-foreground">Time saved</dt>
              <dd className="m-0 tabular-nums">
                {stats ? timeSavedText(stats) : "~0 min"}
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
