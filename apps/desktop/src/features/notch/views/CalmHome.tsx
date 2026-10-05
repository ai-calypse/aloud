import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type Battery,
  type FocusSnapshot,
  type Island,
  type NowPlaying,
  type Quota,
  type Weather,
} from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import { AGENT_NAME } from "@/features/overlay/agentIdentity";
import { recentQuota } from "@/features/overlay/QuotaDashboard";
import { runAction } from "../actions";
import { useNotch } from "../context";
import { positionAt, useArtwork, useNowPlaying } from "../media";
import type { SectionId } from "../sections";
import { useFlip } from "../useFlip";
import { useFocus } from "../useFocus";
import { usePoll } from "../usePoll";
import { useNow, useWeather } from "../useToday";
import { dictateHint, liveOf, plan, weights, type Plan, type TileKey } from "./calm-home";
import {
  AgentsTile,
  BatteryTile,
  CalendarTile,
  DictateTile,
  MediaTile,
  NoteTile,
  TimerTile,
  WeatherTile,
} from "./CalmTiles";
import { SKIP_MS, usePlayerControls } from "./widgets/playback";

/** The charge moves in minutes; half a minute keeps the tile honest without a busy poll. */
const BATTERY_POLL_MS = 30_000;

/** What the board shows; the app fills it from hooks, the tune page from fixtures. */
export type CalmBoardData = {
  plan: Plan;
  /** The clock, for the calendar. The timer has its own (`focusNow`), per second. */
  clock: number;
  focusNow: number;
  track: NowPlaying | null;
  art: string | null;
  positionMs: number;
  snap: FocusSnapshot | null;
  sessions: AgentSession[];
  permissions: AgentPermission[];
  voice: Run | null;
  quota: Quota | null;
  battery: Battery | null | undefined;
  islands: Island[];
  weather: { data: Weather | null; loading: boolean };
  hotkey: string;
};

export type CalmBoardActions = {
  open: (section: SectionId, sub?: string) => void;
  toggle: () => void;
  previous: () => void;
  next: () => void;
  seek: (fraction: number) => void;
  /** An action id from the notch's registry. */
  run: (id: string) => void;
};

/**
 * The eight tiles in their cells. A tile that moves springs there (useFlip);
 * one that changes size crossfades its content (Tile). Order in the DOM is
 * reading order of the plan, which is also the tab order.
 */
export function CalmBoard({ data: d, on }: { data: CalmBoardData; on: CalmBoardActions }) {
  const grid = useRef<HTMLDivElement>(null);
  useFlip(grid, d.plan);
  const p = d.plan;
  const order = (Object.keys(p) as TileKey[]).sort((a, b) => p[a].row - p[b].row || p[a].col - p[b].col);
  const tiles: Record<TileKey, ReactNode> = {
    media: (
      <MediaTile
        placement={p.media}
        track={d.track}
        art={d.art}
        positionMs={d.positionMs}
        onToggle={on.toggle}
        onPrevious={on.previous}
        onNext={on.next}
        onSeek={on.seek}
        onOpen={() => on.open("media", "playing")}
      />
    ),
    timer: (
      <TimerTile
        placement={p.timer}
        snap={d.snap}
        now={d.focusNow}
        onAction={on.run}
        onOpen={() => on.open("focus", "timer")}
      />
    ),
    agents: (
      <AgentsTile
        placement={p.agents}
        sessions={d.sessions}
        permissions={d.permissions}
        voice={d.voice}
        names={AGENT_NAME}
        quota={d.quota}
        onOpen={() => on.open("agents")}
      />
    ),
    battery: <BatteryTile placement={p.battery} battery={d.battery} onOpen={() => on.open("mac", "power")} />,
    calendar: (
      <CalendarTile
        placement={p.calendar}
        islands={d.islands}
        now={d.clock}
        onOpen={() => on.open("today", "calendar")}
      />
    ),
    weather: (
      <WeatherTile
        placement={p.weather}
        data={d.weather.data}
        loading={d.weather.loading}
        onOpen={() => on.open("today", "weather")}
      />
    ),
    dictate: <DictateTile placement={p.dictate} hint={d.hotkey} />,
    note: <NoteTile placement={p.note} onActivate={() => on.run("focus.note")} />,
  };
  return (
    <div ref={grid} role="group" aria-label="Home" className="notch-calm-grid">
      {order.map((key) => (
        <Fragment key={key}>{tiles[key]}</Fragment>
      ))}
    </div>
  );
}

/** The hotkey the Dictate tile teaches, read from Settings each time Home opens. */
function useDictateHint(): string {
  const [hint, setHint] = useState("");
  useEffect(() => {
    try {
      void commands
        .getSettings()
        .then((s) => setHint(dictateHint(s.hotkeys)))
        .catch(() => {});
    } catch {
      /* browser preview: no Tauri, no hotkey to name */
    }
  }, []);
  return hint;
}

/**
 * Now › Overview in the Calm look: the Home board, on the real data the
 * notch already reads (the same hooks the Widgets Home and the Media, Focus
 * and Mac views use). Calendar is the island and the clock, never a script:
 * opening Home must not launch Calendar.app. Plan usage is only what the
 * Usage view already fetched, because reading it prompts for the Keychain.
 */
export function CalmHome() {
  const { navigate, sessions, permissions, islands, agent } = useNotch();
  const { reading, readAt, patch } = useNowPlaying();
  const track = reading.now;
  const art = useArtwork(track);
  const controls = usePlayerControls(track, readAt, patch);
  const { snap, now: focusNow } = useFocus();
  const clock = useNow();
  const battery = usePoll(() => commands.macBattery(), BATTERY_POLL_MS);
  const weather = useWeather();
  const hotkey = useDictateHint();

  // Rust polls once a second; between polls the scrubber needs a clock of its own.
  const playing = track?.state === "playing";
  const [, tick] = useState(0);
  useEffect(() => {
    if (!playing) return;
    const id = window.setInterval(() => tick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [playing]);

  const live = liveOf(sessions, permissions, snap, track, battery);
  const placed = useMemo(
    () => plan(weights(live)),
    // The fields, not the object: `live` is rebuilt each render.
    [live.agents, live.timer, live.mediaPlaying, live.charging, live.batteryPercent],
  );

  // A web video has no previous or next track: those buttons skip ten seconds.
  const web = track?.player === "browser";
  return (
    <CalmBoard
      data={{
        plan: placed,
        clock,
        focusNow,
        track,
        art,
        positionMs: track ? positionAt(track, readAt, performance.now()) : 0,
        snap,
        sessions,
        permissions,
        voice: agent,
        quota: recentQuota(),
        battery,
        islands,
        weather: { data: weather.data, loading: weather.loading },
        hotkey,
      }}
      on={{
        open: navigate,
        toggle: controls.toggle,
        previous: web ? () => controls.jump(-SKIP_MS) : controls.previous,
        next: web ? () => controls.jump(SKIP_MS) : controls.next,
        seek: (fraction) => track && controls.seek(fraction * track.durationMs),
        run: (id) => void runAction(id, { open: navigate }),
      }}
    />
  );
}
