import { useEffect, useMemo, useRef, useState } from "react";
import { FileIcon, NotebookTextIcon, PinIcon } from "lucide-react";
import {
  commands,
  type InstalledApp,
  type LaunchFile,
  type LauncherPrefs,
  type Note,
} from "@/lib/bindings";
import { rank } from "@/lib/fuzzy";
import { cn } from "@/lib/utils";
import { Button, NOTCH_FOCUS } from "@/components/ui/button";
import { AppIcon } from "@/components/notch/app-icon";
import { SearchBox } from "@/components/notch/field";
import { Empty, Label, List, Row, Value } from "@/components/notch/list";
import { useLoaded } from "../use-loaded";

const NO_PREFS: LauncherPrefs = { pins: [], recents: [] };
/** One array for "not loaded yet", so memos keyed on the app list hold. */
const NO_APPS: InstalledApp[] = [];

/** The apps, in the order Spotlight-like launchers show them: pins, recents, the rest. */
function sections(apps: InstalledApp[], prefs: LauncherPrefs) {
  const byName = new Map(apps.map((app) => [app.name, app]));
  const pick = (names: string[]) =>
    names.flatMap((name) => {
      const app = byName.get(name);
      return app ? [app] : [];
    });
  const pinned = pick(prefs.pins);
  const taken = new Set(pinned.map((a) => a.name));
  const recent = pick(prefs.recents)
    .filter((a) => !taken.has(a.name))
    .slice(0, 6);
  recent.forEach((a) => taken.add(a.name));
  return { pinned, recent, rest: apps.filter((a) => !taken.has(a.name)) };
}

/** What an app launch hands back, kept where the panel can show it. */
function useLauncher() {
  const [prefs, setPrefs] = useState<LauncherPrefs>(NO_PREFS);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void commands.launcherPrefs().then(setPrefs);
  }, []);
  const open = async (name: string) => {
    const res = await commands.launcherOpenApp(name);
    if (res.status === "ok") setPrefs(res.data);
    else setError(res.error);
  };
  const pin = async (name: string, pinned: boolean) => {
    const res = await commands.launcherPin(name, pinned);
    if (res.status === "ok") setPrefs(res.data);
  };
  return { prefs, open, pin, error };
}

function AppTile({
  app,
  pinned,
  onOpen,
  onPin,
}: {
  app: InstalledApp;
  pinned: boolean;
  onOpen: () => void;
  onPin: () => void;
}) {
  return (
    <li className="group relative">
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          "flex w-full flex-col items-center gap-1 rounded-notch-row px-1 py-2 font-notch outline-none",
          "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
          "hover:bg-notch-hover active:bg-notch-well",
          NOTCH_FOCUS,
          "focus-visible:outline-offset-[-2px]",
        )}
      >
        <AppIcon icon={app.icon} label="" size="lg" />
        <span className="w-full truncate text-center text-notch-meta text-notch-ink-2">{app.name}</span>
      </button>
      {/* Beside the open button, not inside it: a button in a button is invalid. */}
      <button
        type="button"
        onClick={onPin}
        aria-label={pinned ? `Unpin ${app.name}` : `Pin ${app.name}`}
        aria-pressed={pinned}
        className={cn(
          "absolute top-1 right-1 inline-flex size-5 items-center justify-center rounded-notch-key text-notch-ink-3 outline-none",
          "hover:text-notch-ink focus-visible:opacity-100 group-hover:opacity-100",
          pinned ? "opacity-100" : "opacity-0",
          NOTCH_FOCUS,
        )}
      >
        <PinIcon className={cn("size-3", pinned && "fill-current")} strokeWidth={1.8} />
      </button>
    </li>
  );
}

function Grid({ label, apps, prefs, onOpen, onPin }: {
  label: string;
  apps: InstalledApp[];
  prefs: LauncherPrefs;
  onOpen: (name: string) => void;
  onPin: (name: string, pinned: boolean) => void;
}) {
  if (apps.length === 0) return null;
  return (
    <section className="mt-2 first:mt-0">
      <Label>{label}</Label>
      <ul
        aria-label={label}
        className="m-0 grid list-none grid-cols-[repeat(auto-fill,minmax(84px,1fr))] gap-0.5 p-0"
      >
        {apps.map((app) => {
          const pinned = prefs.pins.includes(app.name);
          return (
            <AppTile
              key={app.name}
              app={app}
              pinned={pinned}
              onOpen={() => onOpen(app.name)}
              onPin={() => onPin(app.name, !pinned)}
            />
          );
        })}
      </ul>
    </section>
  );
}

/** Launch › Apps: every installed app, with pins and recents first. */
export function LaunchApps() {
  const [query, setQuery] = useState("");
  const { prefs, open, pin, error } = useLauncher();
  const { data: apps } = useLoaded(async () => {
    const res = await commands.installedApps();
    return res.status === "ok" ? res.data : null;
  }, []);

  const all = apps ?? NO_APPS;
  const matches = useMemo(() => rank(all, query, (a) => a.name), [all, query]);
  const grouped = useMemo(() => sections(all, prefs), [all, prefs]);

  return (
    <>
      <div
        onKeyDown={(e) => {
          // Enter in the filter opens the best match, as a launcher should.
          if (e.key === "Enter" && query.trim() && matches[0] && e.target instanceof HTMLInputElement) {
            e.preventDefault();
            void open(matches[0].name);
          }
        }}
      >
        <SearchBox label="Find an app" value={query} onChange={setQuery} className="mb-2" />
      </div>
      {error && <p className="m-0 mb-1 text-notch-meta text-notch-danger">{error}</p>}
      {query.trim() ? (
        matches.length === 0 ? (
          <Empty title="No app by that name">Try fewer letters, or the first letters of each word.</Empty>
        ) : (
          <Grid label="Matches" apps={matches} prefs={prefs} onOpen={open} onPin={pin} />
        )
      ) : (
        <>
          <Grid label="Pinned" apps={grouped.pinned} prefs={prefs} onOpen={open} onPin={pin} />
          <Grid label="Recent" apps={grouped.recent} prefs={prefs} onOpen={open} onPin={pin} />
          <Grid label="All apps" apps={grouped.rest} prefs={prefs} onOpen={open} onPin={pin} />
        </>
      )}
    </>
  );
}

type Run = { state: "running" } | { state: "done" } | { state: "failed"; message: string };

/** Launch › Shortcuts: the Apple Shortcuts library, each one a click away. */
export function LaunchShortcuts() {
  const [query, setQuery] = useState("");
  const [runs, setRuns] = useState<Record<string, Run>>({});
  const { data: names } = useLoaded(async () => {
    const res = await commands.shortcutList();
    return res.status === "ok" ? res.data : null;
  }, []);

  const run = async (name: string) => {
    setRuns((r) => ({ ...r, [name]: { state: "running" } }));
    const res = await commands.shortcutRun(name);
    setRuns((r) => ({
      ...r,
      [name]: res.status === "ok" ? { state: "done" } : { state: "failed", message: res.error },
    }));
  };

  if (names === null) return null;
  if (names.length === 0) {
    return (
      <Empty
        title="No shortcuts yet"
        action={
          <Button variant="notch" size="notch" onClick={() => void commands.launcherOpenApp("Shortcuts")}>
            Open Shortcuts
          </Button>
        }
      >
        Shortcuts you make in the Shortcuts app show up here, one click to run.
      </Empty>
    );
  }

  const shown = rank(names, query, (n) => n);
  return (
    <>
      {names.length > 6 && (
        <SearchBox label="Find a shortcut" value={query} onChange={setQuery} className="mb-2" />
      )}
      <Label>Shortcuts</Label>
      <List label="Shortcuts">
        {shown.map((name) => {
          const status = runs[name];
          return (
            <Row
              key={name}
              title={name}
              subtitle={
                status?.state === "failed" ? (
                  <span className="text-notch-danger">Failed: {status.message}</span>
                ) : undefined
              }
              trailing={
                <span className="flex shrink-0 items-center gap-2">
                  {status?.state === "running" && <Value>Running…</Value>}
                  {status?.state === "done" && <Value className="text-notch-ok">Done</Value>}
                  <Button
                    variant="notch-quiet"
                    size="notch"
                    disabled={status?.state === "running"}
                    onClick={() => void run(name)}
                  >
                    Run
                  </Button>
                </span>
              }
            />
          );
        })}
      </List>
    </>
  );
}

type Hit = {
  key: string;
  group: "Apps" | "Notes" | "Files" | "Recent";
  title: string;
  subtitle?: string;
  lead: React.ReactNode;
  run: () => void;
};

const FILE_LEAD = (
  <span className="inline-flex size-[26px] shrink-0 items-center justify-center rounded-notch-control bg-notch-well text-notch-ink-2">
    <FileIcon className="size-3.5" strokeWidth={1.8} aria-hidden />
  </span>
);
const NOTE_LEAD = (
  <span className="inline-flex size-[26px] shrink-0 items-center justify-center rounded-notch-control bg-notch-well text-notch-ink-2">
    <NotebookTextIcon className="size-3.5" strokeWidth={1.8} aria-hidden />
  </span>
);

/**
 * Launch › Search: apps, notes and files in one box. Enter opens the first
 * hit. Translation is not here: macOS exposes it only through a SwiftUI
 * session, and a keyless web service would send what the user typed off the Mac.
 */
export function LaunchSearch() {
  const [query, setQuery] = useState("");
  const { prefs, open } = useLauncher();
  const { data: apps } = useLoaded(async () => {
    const res = await commands.installedApps();
    return res.status === "ok" ? res.data : null;
  }, []);
  const [notes, setNotes] = useState<Note[]>([]);
  const [files, setFiles] = useState<LaunchFile[]>([]);
  const latest = useRef(0);
  const q = query.trim();

  // Notes and files are a disk read each; wait for the typing to pause. An
  // answer to an older query is dropped.
  useEffect(() => {
    const ticket = ++latest.current;
    const timer = setTimeout(
      () => {
        void (async () => {
          const [found, recent, f] = await Promise.all([
            q ? commands.searchNotes(q) : null,
            q ? null : commands.recentNotes(3),
            q ? commands.launcherFiles(q) : commands.launcherRecentFiles(),
          ]);
          if (ticket !== latest.current) return;
          const matched = found?.status === "ok" ? found.data.map((r) => r.note) : [];
          setNotes((recent?.status === "ok" ? recent.data : matched).slice(0, 4));
          setFiles(f.status === "ok" ? f.data.slice(0, 5) : []);
        })();
      },
      q ? 180 : 0,
    );
    return () => clearTimeout(timer);
  }, [q]);

  const hits = useMemo<Hit[]>(() => {
    const all = apps ?? NO_APPS;
    const appHits = (q ? rank(all, q, (a) => a.name) : all.filter((a) => prefs.recents.includes(a.name)))
      .slice(0, q ? 5 : 4)
      .map<Hit>((app) => ({
        key: `app:${app.name}`,
        group: q ? "Apps" : "Recent",
        title: app.name,
        subtitle: "App",
        lead: <AppIcon icon={app.icon} label="" />,
        run: () => void open(app.name),
      }));
    const noteHits = notes.map<Hit>((note) => ({
      key: `note:${note.path}`,
      group: "Notes",
      title: note.title,
      subtitle: "Note",
      lead: NOTE_LEAD,
      run: () => void commands.launcherOpenPath(note.path),
    }));
    const fileHits = files.map<Hit>((file) => ({
      key: `file:${file.path}`,
      group: "Files",
      title: file.name,
      subtitle: file.folder,
      lead: FILE_LEAD,
      run: () => void commands.launcherOpenPath(file.path),
    }));
    return [...appHits, ...noteHits, ...fileHits];
  }, [apps, prefs.recents, notes, files, q, open]);

  const groups = (["Recent", "Apps", "Notes", "Files"] as const)
    .map((group) => ({ group, items: hits.filter((h) => h.group === group) }))
    .filter((g) => g.items.length > 0);

  return (
    <>
      <div
        onKeyDown={(e) => {
          if (e.key === "Enter" && hits[0] && e.target instanceof HTMLInputElement) {
            e.preventDefault();
            hits[0].run();
          }
        }}
      >
        <SearchBox label="Search apps, notes and files" value={query} onChange={setQuery} className="mb-2" />
      </div>
      {groups.length === 0 ? (
        <Empty title={q ? "Nothing found" : "Nothing recent"}>
          {q
            ? "Searched your apps, Aloud notes and the files in your home folder."
            : "Apps you open from here, your latest notes and recently opened files show up here."}
        </Empty>
      ) : (
        groups.map(({ group, items }) => (
          <section key={group} className="mt-2 first:mt-0">
            <Label>{group}</Label>
            <List label={group}>
              {items.map((hit) => (
                <Row
                  key={hit.key}
                  lead={hit.lead}
                  title={hit.title}
                  subtitle={hit.subtitle}
                  onClick={hit.run}
                />
              ))}
            </List>
          </section>
        ))
      )}
    </>
  );
}
