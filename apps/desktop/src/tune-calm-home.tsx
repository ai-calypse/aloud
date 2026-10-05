import { useState, type ReactNode } from "react";
import { SearchIcon, SettingsIcon } from "lucide-react";
import { Button } from "./components/ui/button";
import { Kbd } from "./components/notch/controls";
import { SectionTabs } from "./components/notch/section-tabs";
import type { Run } from "./lib/agent-steps";
import type {
  AgentPermission,
  AgentSession,
  Battery,
  FocusSnapshot,
  Island,
  NowPlaying,
  Quota,
  Weather,
} from "./lib/bindings";
import { SECTIONS } from "./features/notch/registry";
import { liveOf, plan, weights } from "./features/notch/views/calm-home";
import { CalmBoard, type CalmBoardData } from "./features/notch/views/CalmHome";
import { BatteryTile } from "./features/notch/views/CalmTiles";

/**
 * "Shell · Home · Calm": the Calm Home board in the panel's chrome, from
 * labelled fixtures. The toggles are the prototype's "Live items": turn the
 * agents, the timer or the music on and watch the tiles reflow and change
 * size; never the app's data.
 */

const MIN = 60_000;
const CLOCK = new Date(2026, 9, 4, 14, 5).getTime();

const TRACK: NowPlaying = {
  player: "spotify",
  source: { app: "Spotify", host: null, tab: null },
  state: "playing",
  title: "Night Drive",
  artist: "Halcyon",
  album: "Moon Atlas",
  durationMs: 222_000,
  positionMs: 102_000,
  speedPct: 100,
  artworkUrl: null,
  song: null,
} as NowPlaying;

// A cover drawn in place of a file: fixtures carry no images.
const ART =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#2a1740"/><circle cx="50" cy="46" r="27" fill="#ff6b8b"/><rect y="52" width="100" height="3" fill="#2a1740"/><rect y="60" width="100" height="4" fill="#2a1740"/><rect y="69" width="100" height="5" fill="#2a1740"/><rect y="78" width="100" height="22" fill="#4a2466"/></svg>',
  );

const WEATHER: Weather = {
  place: "Oakland",
  temperature: 21,
  feelsLike: 20,
  sky: "partlyCloudy",
  summary: "Partly cloudy",
  isDay: true,
  days: [
    { day: "2026-10-04", sky: "partlyCloudy", high: 24, low: 15 },
    { day: "2026-10-05", sky: "clear", high: 26, low: 16 },
    { day: "2026-10-06", sky: "cloudy", high: 22, low: 14 },
    { day: "2026-10-07", sky: "rain", high: 19, low: 13 },
    { day: "2026-10-08", sky: "partlyCloudy", high: 21, low: 14 },
  ],
};

const SNAP: FocusSnapshot = {
  awake: { active: false, until: null, minutes: null },
  timer: {
    mode: "pomodoro",
    phase: "focus",
    status: "running",
    round: 2,
    rounds: 4,
    preset: { focus: 25, rest: 5 },
    at: CLOCK + 18 * MIN + 42_000,
    heldMs: 0,
    totalMs: 25 * MIN,
  },
} as FocusSnapshot;

const SESSIONS = [
  { id: "a", agent: "claude", title: "tauri", prompt: null, cwd: "/Users/me/aloud-tauri", tool: null, busy: false },
  { id: "b", agent: "codex", title: "site", prompt: null, cwd: "/Users/me/website", tool: "cargo test", busy: true },
] as AgentSession[];
const PERMISSIONS = [{ id: "a", sessionId: "a", agent: "claude", tool: "Bash" }] as AgentPermission[];
const VOICE: Run = { heard: "email David", steps: [], finished: "Sent" };
const QUOTA = { fiveHour: { percent: 62, resetsAt: null }, sevenDay: null, sevenDayOpus: null, extraUsagePercent: null } as Quota;

const NEXT: Island = {
  id: "calendar:next",
  kind: "live",
  priority: "normal",
  icon: "calendar",
  title: "Design sync",
  detail: "in 25 min",
  actions: [],
  expires: null,
  dismissible: false,
};

type Scene = {
  media: boolean;
  timer: boolean;
  agents: boolean;
  charging: boolean;
  event: boolean;
  weather: boolean;
  battery: number | null;
};

const IDLE: Scene = { media: false, timer: false, agents: false, charging: false, event: false, weather: true, battery: 82 };
const LIVE: Scene = { ...IDLE, media: true, timer: true, agents: true, event: true };

function dataFor(s: Scene): CalmBoardData {
  const battery: Battery | null =
    s.battery === null
      ? null
      : { percent: s.battery, state: s.charging ? "charging" : "discharging", minutes: 300, cycles: 120, health: 94 };
  const track = s.media ? TRACK : null;
  const snap = s.timer ? SNAP : null;
  const sessions = s.agents ? SESSIONS : [];
  const permissions = s.agents ? PERMISSIONS : [];
  return {
    plan: plan(weights(liveOf(sessions, permissions, snap, track, battery))),
    clock: CLOCK,
    focusNow: CLOCK,
    track,
    art: s.media ? ART : null,
    positionMs: TRACK.positionMs,
    snap,
    sessions,
    permissions,
    voice: s.agents ? VOICE : null,
    quota: s.agents ? QUOTA : null,
    battery,
    islands: s.event ? [NEXT] : [],
    weather: { data: s.weather ? WEATHER : null, loading: false },
    hotkey: "Hold Fn 🌐",
  };
}

const noop = () => {};
const ON = { open: noop, toggle: noop, previous: noop, next: noop, seek: noop, run: noop };

const TABS = SECTIONS.map((s) => ({ id: s.id, label: s.name, shortcut: undefined }));

/** The panel as the shell draws it for Calm, around a board of fixtures. */
function Panel({ children, caption }: { children: ReactNode; caption: string }) {
  return (
    <div className="w-[780px] overflow-hidden rounded-b-[24px] bg-notch font-notch text-notch-ink">
      <header className="flex items-center gap-1 border-b border-notch-sep px-3.5">
        <SectionTabs
          className="flex-1"
          panelId="calm-fixture"
          value="now"
          onChange={noop}
          items={TABS}
        />
        <Button variant="notch-quiet" size="notch" aria-label="Search sections, features and actions">
          <SearchIcon />
          <Kbd>⌘K</Kbd>
        </Button>
        <Button variant="notch-quiet" size="notch-icon" aria-label="Settings">
          <SettingsIcon />
        </Button>
      </header>
      <div id="calm-fixture" className="px-4 pt-2 pb-3">
        {children}
      </div>
      <footer className="flex items-center justify-between gap-3.5 border-t border-notch-sep px-4 py-2 text-notch-label text-notch-ink-3">
        <span className="truncate">{caption}</span>
        <span className="flex shrink-0 items-center gap-3.5">
          <span className="flex items-center gap-1.5">
            <Kbd>⌘1–9</Kbd> sections
          </span>
          <span className="flex items-center gap-1.5">
            <Kbd>esc</Kbd> close
          </span>
        </span>
      </footer>
    </div>
  );
}

const TOGGLES: [keyof Scene, string][] = [
  ["media", "Music"],
  ["timer", "Timer"],
  ["agents", "Agents"],
  ["charging", "Charging"],
  ["event", "Next event"],
  ["weather", "Weather"],
];

export function CalmHomeGallery({ initial = "live" }: { initial?: "live" | "idle" | "off" }) {
  const [scene, setScene] = useState<Scene>(
    initial === "live" ? LIVE : initial === "off" ? { ...IDLE, weather: false, battery: null } : IDLE,
  );
  const toggle = (k: keyof Scene) => setScene((s) => ({ ...s, [k]: !s[k] }));

  return (
    <div className="grid gap-3" data-calm-home-fixture>
      <div className="flex flex-wrap items-center gap-2 text-notch-meta text-notch-ink-2">
        {TOGGLES.map(([k, label]) => (
          <button
            key={k}
            type="button"
            aria-pressed={!!scene[k]}
            data-toggle={k}
            onClick={() => toggle(k)}
            className="rounded-full bg-notch-well px-3 py-1 aria-pressed:bg-notch-raised aria-pressed:text-notch-ink"
          >
            {label}
          </button>
        ))}
        {[100, 82, 18, 9].map((p) => (
          <button
            key={p}
            type="button"
            data-battery={p}
            onClick={() => setScene((s) => ({ ...s, battery: p, charging: false }))}
            className="rounded-full bg-notch-well px-3 py-1 aria-pressed:bg-notch-raised aria-pressed:text-notch-ink"
            aria-pressed={scene.battery === p && !scene.charging}
          >
            {p}%
          </button>
        ))}
      </div>
      <Panel caption="Home · ranked by what is live, biggest first">
        <CalmBoard data={dataFor(scene)} on={ON} />
      </Panel>
      {/* The 1x1 battery, which the ranking only picks when Home is crowded:
          "100%" is the widest it ever has to fit. */}
      <Panel caption="Battery · 1x1 at full, good and critical">
        <div className="notch-calm-grid" data-battery-small>
          {[100, 82, 9].map((percent, i) => (
            <BatteryTile
              key={percent}
              placement={{ id: `battery-${percent}`, col: i + 1, row: 1, w: 1, h: 1, size: "S" }}
              battery={{ percent, state: percent === 100 ? "full" : "discharging", minutes: 300, cycles: 120, health: 94 }}
            />
          ))}
        </div>
      </Panel>
    </div>
  );
}
