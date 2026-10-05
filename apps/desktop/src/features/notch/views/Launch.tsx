import { useEffect, useMemo, useRef, useState } from "react";
import { FileIcon, NotebookTextIcon } from "lucide-react";
import {
  commands,
  type InstalledApp,
  type LaunchFile,
  type LauncherPrefs,
  type Note,
} from "@/lib/bindings";
import { rank } from "@/lib/fuzzy";
import { AppIcon } from "@/components/notch/app-icon";
import { SearchBox } from "@/components/notch/field";
import { Empty, Label, List, Row } from "@/components/notch/list";
import { useLoaded } from "../use-loaded";

const NO_PREFS: LauncherPrefs = { pins: [], recents: [] };
/** One array for "not loaded yet", so memos keyed on the app list hold. */
export const NO_APPS: InstalledApp[] = [];

/** The apps, in the order Spotlight-like launchers show them: pins, recents, the rest. */
export function sections(apps: InstalledApp[], prefs: LauncherPrefs) {
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
export function useLauncher() {
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

export type Run = { state: "running" } | { state: "done" } | { state: "failed"; message: string };

/** The Shortcuts library and what each run last did; shared with the Shortcuts board. */
export function useShortcuts() {
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
  return { names, runs, run };
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
