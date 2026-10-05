import { useMemo, useState } from "react";
import { AppWindowIcon, LayersIcon, LoaderCircleIcon, PinIcon } from "lucide-react";
import { commands, type InstalledApp, type LauncherPrefs } from "@/lib/bindings";
import { rank } from "@/lib/fuzzy";
import { cn } from "@/lib/utils";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { AppIcon } from "@/components/notch/app-icon";
import { WidgetBoard, WidgetFrame, type WidgetSize } from "@/components/notch/widgets";
import { useLoaded } from "../../use-loaded";
import { NO_APPS, sections, useLauncher, useShortcuts, type Run } from "../Launch";
import { chunk, runWord, shortcutRole } from "./launch-logic";
import { Idle, Pill, WidgetSearch } from "./agents-parts";

/* ---------------------------------------------------------------------------
 * Launch › Apps
 * ------------------------------------------------------------------------- */

/**
 * Launch › Apps in the widgets look: App Store's blue tile for what is
 * pinned, Siri Suggestions' vibrant one for what was opened lately, and a
 * Large tile for every app. The filter and Enter-opens-the-best-match are the
 * calm view's: typing swaps the board for the matches, with the one Enter
 * would open called out beside them.
 */
export function WidgetApps() {
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
        <WidgetSearch label="Find an app" value={query} onChange={setQuery} />
      </div>
      {error && (
        <p data-w-role="move" className="notch-board-bar m-0 mb-2 text-notch-w-meta font-medium text-widget-acc-ink">
          {error}
        </p>
      )}
      <AppsBoard
        searching={query.trim() !== ""}
        matches={matches}
        pinned={grouped.pinned}
        recent={grouped.recent}
        rest={grouped.rest}
        prefs={prefs}
        onOpen={(name) => void open(name)}
        onPin={(name, pinned) => void pin(name, pinned)}
      />
    </>
  );
}

export function AppsBoard({
  searching,
  matches,
  pinned,
  recent,
  rest,
  prefs,
  onOpen,
  onPin,
}: {
  searching: boolean;
  matches: InstalledApp[];
  pinned: InstalledApp[];
  recent: InstalledApp[];
  rest: InstalledApp[];
  prefs: LauncherPrefs;
  onOpen: (name: string) => void;
  onPin: (name: string, pinned: boolean) => void;
}) {
  const grid = { prefs, onOpen, onPin };
  if (searching) {
    return (
      <WidgetBoard label="Matches">
        {matches.length === 0 ? (
          <WidgetFrame size="medium" label="Matches">
            <Idle icon={<AppWindowIcon strokeWidth={1.6} />} title="No app by that name">
              Try fewer letters, or the first letters of each word.
            </Idle>
          </WidgetFrame>
        ) : (
          <>
            <AppsTile size="large" label="Matches" apps={matches} {...grid} />
            <TopHit app={matches[0]} onOpen={() => onOpen(matches[0].name)} />
          </>
        )}
      </WidgetBoard>
    );
  }
  // Pins and recents stack on the left and the whole list takes the right, fixed in
  // place so it does not move when a group is empty; with neither it centres.
  const lead = pinned.length > 0 || recent.length > 0;
  return (
    <WidgetBoard label="Apps">
      {pinned.length > 0 && <AppsTile size="medium" label="Pinned" ground="accent" accent="blue" apps={pinned} {...grid} />}
      {recent.length > 0 && <AppsTile size="medium" label="Recent" ground="vibrant" apps={recent} {...grid} />}
      {rest.length > 0 && (
        <AppsTile
          size="large"
          label="All apps"
          apps={rest}
          className={lead ? "col-start-3 row-start-1" : "col-start-2"}
          {...grid}
        />
      )}
    </WidgetBoard>
  );
}

function AppsTile({
  size,
  label,
  apps,
  prefs,
  onOpen,
  onPin,
  ground,
  accent,
  className,
}: {
  size: WidgetSize;
  label: string;
  apps: InstalledApp[];
  prefs: LauncherPrefs;
  onOpen: (name: string) => void;
  onPin: (name: string, pinned: boolean) => void;
  ground?: "tile" | "vibrant" | "deep" | "accent";
  accent?: "blue";
  className?: string;
}) {
  return (
    <WidgetFrame size={size} label={label} ground={ground} accent={accent} className={className}>
      {/* Two rows fit a Medium; a longer list scrolls inside its tile. */}
      <ul
        aria-label={label}
        className="m-0 grid h-full list-none grid-cols-5 content-start gap-x-1.5 gap-y-1 overflow-y-auto overscroll-contain p-3"
      >
        {apps.map((app) => {
          const pinned = prefs.pins.includes(app.name);
          return (
            <AppCell
              key={app.name}
              app={app}
              pinned={pinned}
              onOpen={() => onOpen(app.name)}
              onPin={() => onPin(app.name, !pinned)}
            />
          );
        })}
      </ul>
    </WidgetFrame>
  );
}

function AppCell({
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
          "flex w-full flex-col items-center gap-0.5 rounded-notch-row px-0.5 py-1 outline-none",
          "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
          "hover:bg-widget-ink/10 active:bg-widget-ink/20",
          NOTCH_FOCUS,
          "focus-visible:outline-offset-[-2px]",
        )}
      >
        <AppIcon icon={app.icon} label="" size="lg" className="size-10 rounded-notch-row" />
        <span className="w-full truncate text-center text-notch-w-over font-medium">{app.name}</span>
      </button>
      {/* Beside the open button, not inside it: a button in a button is invalid. */}
      <button
        type="button"
        onClick={onPin}
        aria-label={pinned ? `Unpin ${app.name}` : `Pin ${app.name}`}
        aria-pressed={pinned}
        className={cn(
          "absolute top-0 right-0 inline-flex size-5 items-center justify-center rounded-full bg-widget-tile text-widget-ink outline-none",
          "focus-visible:opacity-100 group-hover:opacity-100",
          pinned ? "opacity-100" : "opacity-0",
          NOTCH_FOCUS,
        )}
      >
        <PinIcon className={cn("size-3", pinned && "fill-current")} strokeWidth={1.8} aria-hidden />
      </button>
    </li>
  );
}

/** The match Enter opens, said aloud and openable by pointer, as Spotlight's top hit. */
function TopHit({ app, onOpen }: { app: InstalledApp; onOpen: () => void }) {
  return (
    <WidgetFrame size="medium" label="Top hit" ground="vibrant">
      <div className="flex h-full items-center gap-3.5 p-3.5">
        <AppIcon icon={app.icon} label="" size="lg" className="size-16 rounded-notch-panel" />
        <div className="flex min-w-0 flex-1 flex-col items-start gap-0.5">
          <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
            Return opens
          </span>
          <span className="max-w-full truncate text-notch-w-headline leading-tight font-semibold">{app.name}</span>
          <Pill className="mt-2" onClick={onOpen}>
            Open
          </Pill>
        </div>
      </div>
    </WidgetFrame>
  );
}

/* ---------------------------------------------------------------------------
 * Launch › Shortcuts
 * ------------------------------------------------------------------------- */

/** Four shortcuts to a Medium tile, as Apple's Shortcuts widget holds them. */
const PER_TILE = 4;

/**
 * Launch › Shortcuts in the widgets look: a coloured tile per shortcut, four to
 * a Medium widget, each saying Running…, Done or Failed in words. Tapping runs
 * it. A failure's message, which a tile has no room for, is read under the board.
 */
export function WidgetShortcuts() {
  const [query, setQuery] = useState("");
  const { names, runs, run } = useShortcuts();
  if (names === null) return null;
  return (
    <ShortcutsBoard
      names={names}
      shown={rank(names, query, (n) => n)}
      runs={runs}
      query={query}
      onQuery={setQuery}
      onRun={(name) => void run(name)}
    />
  );
}

export function ShortcutsBoard({
  names,
  shown,
  runs,
  query,
  onQuery,
  onRun,
}: {
  names: string[];
  shown: string[];
  runs: Record<string, Run>;
  query: string;
  onQuery: (query: string) => void;
  onRun: (name: string) => void;
}) {
  if (names.length === 0) {
    return (
      <WidgetBoard label="Shortcuts">
        <WidgetFrame size="medium" label="Shortcuts">
          <Idle icon={<LayersIcon strokeWidth={1.6} />} title="No shortcuts yet">
            Shortcuts you make in the Shortcuts app show up here, one tap to run.
            <span className="mt-2 flex">
              <Pill onClick={() => void commands.launcherOpenApp("Shortcuts")}>Open Shortcuts</Pill>
            </span>
          </Idle>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  const failures = Object.entries(runs).flatMap(([name, r]) => (r.state === "failed" ? [[name, r.message]] : []));
  return (
    <>
      {names.length > 6 && <WidgetSearch label="Find a shortcut" value={query} onChange={onQuery} />}
      <WidgetBoard label="Shortcuts">
        {shown.length === 0 ? (
          <WidgetFrame size="medium" label="Shortcuts">
            <Idle icon={<LayersIcon strokeWidth={1.6} />} title="No shortcut by that name">
              Try fewer letters, or the first letters of each word.
            </Idle>
          </WidgetFrame>
        ) : (
          chunk(shown, PER_TILE).map((group, i) => (
            <WidgetFrame key={group[0]} size="medium" label={i === 0 ? "Shortcuts" : "More shortcuts"} ground="deep">
              <ul
                aria-label={i === 0 ? "Shortcuts" : "More shortcuts"}
                className="m-0 grid h-full list-none grid-cols-2 grid-rows-2 gap-2 p-2"
              >
                {group.map((name) => (
                  <li key={name} className="flex min-w-0">
                    <ShortcutTile name={name} run={runs[name]} onRun={() => onRun(name)} />
                  </li>
                ))}
              </ul>
            </WidgetFrame>
          ))
        )}
      </WidgetBoard>
      {failures.length > 0 && (
        <div aria-live="polite" className="notch-board-bar mt-2 grid gap-0.5">
          {failures.map(([name, message]) => (
            <p key={name} data-w-role="move" className="m-0 text-notch-w-meta font-medium text-widget-acc-ink">
              {name} failed: {message}
            </p>
          ))}
        </div>
      )}
    </>
  );
}

function ShortcutTile({ name, run, onRun }: { name: string; run: Run | undefined; onRun: () => void }) {
  const running = run?.state === "running";
  const word = runWord(run);
  return (
    <button
      type="button"
      data-w-role={shortcutRole(name)}
      disabled={running}
      onClick={onRun}
      title={name}
      className={cn(
        "flex w-full min-w-0 flex-col justify-between rounded-2xl bg-widget-fill p-2.5 text-left text-widget-ink outline-none",
        "transition-[filter] duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:brightness-110 active:brightness-90",
        NOTCH_FOCUS,
      )}
    >
      <span className="flex items-start justify-between gap-1">
        {running ? (
          <LoaderCircleIcon className="size-5 animate-spin" strokeWidth={1.8} aria-hidden />
        ) : (
          <LayersIcon className="size-5" strokeWidth={1.8} aria-hidden />
        )}
        {word && <span className="text-notch-w-meta leading-tight font-semibold">{word}</span>}
      </span>
      <span className="truncate text-notch-w-title leading-tight font-semibold">{name}</span>
    </button>
  );
}
