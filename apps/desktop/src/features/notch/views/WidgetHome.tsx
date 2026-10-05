import type { ReactNode } from "react";
import {
  BatteryChargingIcon,
  BatteryIcon,
  BotIcon,
  CalendarIcon,
  MoonIcon,
  MusicIcon,
  PauseIcon,
  PlayIcon,
  TimerIcon,
} from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import {
  EventCard,
  formatNumeral,
  Numeral,
  Ring,
  WidgetBoard,
  WidgetFrame,
} from "@/components/notch/widgets";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type Battery,
  type FocusSnapshot,
  type Island,
  type NowPlaying,
  type Weather,
} from "@/lib/bindings";
import { degrees } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useNotch } from "../context";
import { useNowPlaying, useArtwork } from "../media";
import { useFocus } from "../useFocus";
import { usePoll } from "../usePoll";
import { useNow, useWeather } from "../useToday";
import { SKY } from "./Today";
import { agentsWidget, batteryWidget, calendarWidget, timerWidget } from "./widget-home";

/** The charge moves in minutes; half a minute keeps the tile honest without a busy poll. */
const BATTERY_POLL_MS = 30_000;

/**
 * Now › Overview in the widgets look: the Home board. Six Apple-style tiles
 * drawn from what the notch already knows (Now Playing, weather, agents, the
 * calendar's next event, the focus timer, the battery), each saying plainly
 * when its source has nothing: an empty tile, never a made-up one. Each tile
 * opens the section it summarises.
 *
 * Fixed layout, not a ranking: the calm Overview ranks what is live; widgets
 * cannot be resized, so a board is a grid (see DESIGN.md › Looks).
 */
export function WidgetHome() {
  const { navigate } = useNotch();
  return (
    <WidgetBoard label="Home">
      <MusicWidget onOpen={() => navigate("media", "playing")} />
      <WeatherWidget onOpen={() => navigate("today", "weather")} />
      <AgentsWidget onOpen={() => navigate("agents")} />
      <CalendarWidget onOpen={() => navigate("today", "calendar")} />
      <TimerWidget onOpen={() => navigate("focus", "timer")} />
      <BatteryWidget onOpen={() => navigate("mac", "power")} />
    </WidgetBoard>
  );
}

/** A quiet line for a tile whose source has nothing, with the glyph that names the source. */
function Idle({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="flex h-full flex-col justify-between p-3.5 text-widget-ink-2">
      <span aria-hidden className="[&_svg]:size-6">
        {icon}
      </span>
      <span className="text-notch-w-title leading-tight font-medium">{children}</span>
    </div>
  );
}

function send(now: NowPlaying) {
  try {
    void commands.mediaControl(now.player, "toggle", null);
  } catch {
    /* browser preview: nothing to drive */
  }
}

function MusicWidget({ onOpen }: { onOpen: () => void }) {
  const { reading, patch } = useNowPlaying();
  const { now } = reading;
  const art = useArtwork(now);
  return (
    <MusicTile
      now={now}
      art={art}
      onOpen={onOpen}
      onToggle={() => {
        if (!now) return;
        patch((n) => ({ ...n, state: n.state === "playing" ? "paused" : "playing" }));
        send(now);
      }}
    />
  );
}

export function MusicTile({
  now,
  art,
  onOpen,
  onToggle,
}: {
  now: NowPlaying | null;
  art: string | null;
  onOpen?: () => void;
  onToggle: () => void;
}) {
  if (!now) {
    return (
      <WidgetFrame size="medium" label="Music" onActivate={onOpen}>
        <Idle icon={<MusicIcon strokeWidth={1.6} />}>Nothing playing</Idle>
      </WidgetFrame>
    );
  }
  const playing = now.state === "playing";
  return (
    <WidgetFrame size="medium" label="Music" accent="media" ground="accent" onActivate={onOpen}>
      <div className="flex h-full gap-3.5 p-3.5">
        {art ? (
          <img
            src={art}
            alt=""
            draggable={false}
            className="size-32 shrink-0 rounded-notch-row object-cover"
          />
        ) : (
          <span
            aria-hidden
            className="grid size-32 shrink-0 place-items-center rounded-notch-row bg-widget-tile text-widget-ink-2"
          >
            <MusicIcon className="size-8" strokeWidth={1.4} />
          </span>
        )}
        <div className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
          <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink/80 uppercase">
            {playing ? "Now playing" : "Paused"}
          </span>
          <span className="truncate text-notch-w-headline leading-tight font-semibold">{now.title}</span>
          <span className="truncate text-notch-w-body leading-tight text-widget-ink/85">{now.artist}</span>
          {/* A white pill with dark words: Apple's translucent one is under 3:1. */}
          <button
            type="button"
            onClick={onToggle}
            className={cn(
              NOTCH_FOCUS,
              "mt-2 flex h-7.5 w-fit items-center gap-1.5 rounded-full bg-widget-ink px-3.5 text-notch-w-title font-semibold text-widget-on",
            )}
          >
            {playing ? <PauseIcon className="size-3" fill="currentColor" /> : <PlayIcon className="size-3" fill="currentColor" />}
            {playing ? "Pause" : "Play"}
          </button>
        </div>
      </div>
    </WidgetFrame>
  );
}

function WeatherWidget({ onOpen }: { onOpen: () => void }) {
  const { data, loading } = useWeather();
  return <WeatherTile data={data} loading={loading} onOpen={onOpen} />;
}

export function WeatherTile({
  data,
  loading,
  onOpen,
}: {
  data: Weather | null;
  loading: boolean;
  onOpen?: () => void;
}) {
  if (!data) {
    return (
      <WidgetFrame size="small" label="Weather" onActivate={onOpen}>
        <Idle icon={<SKY.partlyCloudy strokeWidth={1.6} />}>
          {loading ? "Checking the sky" : "Weather unavailable"}
        </Idle>
      </WidgetFrame>
    );
  }
  const Sky = data.sky === "clear" && !data.isDay ? MoonIcon : SKY[data.sky];
  const today = data.days[0];
  return (
    <WidgetFrame size="small" label="Weather" accent="blue" ground="accent" onActivate={onOpen}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <div>
          <div className="truncate text-notch-w-body leading-tight font-medium">{data.place}</div>
          <Numeral text={degrees(data.temperature)} size="l" />
        </div>
        <div className="text-notch-w-title leading-snug font-medium">
          <div className="flex items-center gap-1.5">
            <Sky aria-hidden className="size-4.5 shrink-0 text-widget-ink" strokeWidth={1.8} />
            <span className="truncate">{data.summary}</span>
          </div>
          {today && (
            <div className="text-widget-ink/85">
              H:{degrees(today.high)} L:{degrees(today.low)}
            </div>
          )}
        </div>
      </div>
    </WidgetFrame>
  );
}

function AgentsWidget({ onOpen }: { onOpen: () => void }) {
  const { sessions, permissions } = useNotch();
  return <AgentsTile sessions={sessions} permissions={permissions} onOpen={onOpen} />;
}

export function AgentsTile({
  sessions,
  permissions,
  onOpen,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  onOpen?: () => void;
}) {
  const a = agentsWidget(sessions, permissions);
  if (a.count === null) {
    return (
      <WidgetFrame size="small" label="Agents" onActivate={onOpen}>
        <Idle icon={<BotIcon strokeWidth={1.6} />}>{a.word === "all idle" ? "Agents are idle" : "No agents running"}</Idle>
      </WidgetFrame>
    );
  }
  return (
    <WidgetFrame size="small" label="Agents" accent="agent" live onActivate={onOpen}>
      <div className="flex h-full flex-col justify-between p-3.5 text-widget-acc-ink">
        <BotIcon aria-hidden className="size-6" strokeWidth={1.6} />
        <div>
          <Numeral text={formatNumeral(a.count)} size="xl" roll className="text-widget-acc-ink" />
          <div className="text-notch-w-title font-semibold">{a.word}</div>
        </div>
      </div>
    </WidgetFrame>
  );
}

function CalendarWidget({ onOpen }: { onOpen: () => void }) {
  const { islands } = useNotch();
  return <CalendarTile islands={islands} now={useNow()} onOpen={onOpen} />;
}

export function CalendarTile({
  islands,
  now,
  onOpen,
}: {
  islands: Island[];
  now: number;
  onOpen?: () => void;
}) {
  const c = calendarWidget(islands, now);
  return (
    <WidgetFrame size="medium" label="Calendar" onActivate={onOpen}>
      <div className="flex h-full gap-3.5 p-3.5">
        <div className="min-w-0 flex-1" data-w-role="calendar">
          <div className="text-notch-w-over font-semibold tracking-wide text-widget-acc-ink uppercase">
            {c.weekday}
          </div>
          <Numeral text={c.day} size="s" />
        </div>
        <div className="flex min-w-0 flex-1 flex-col justify-end gap-1.5">
          {c.next ? (
            <>
              <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
                Next
              </span>
              <EventCard title={c.next.title} time={c.next.when} tone="blue" />
            </>
          ) : (
            <span className="flex items-center gap-1.5 text-notch-w-meta leading-tight font-medium text-widget-ink-2">
              <CalendarIcon aria-hidden className="size-3.5 shrink-0" />
              Open for today&apos;s events
            </span>
          )}
        </div>
      </div>
    </WidgetFrame>
  );
}

function TimerWidget({ onOpen }: { onOpen: () => void }) {
  const { snap, now } = useFocus();
  return <TimerTile snap={snap} now={now} onOpen={onOpen} />;
}

export function TimerTile({
  snap,
  now,
  onOpen,
}: {
  snap: FocusSnapshot | null;
  now: number;
  onOpen?: () => void;
}) {
  const t = timerWidget(snap, now);
  if (t.idle) {
    return (
      <WidgetFrame size="small" label="Timer" onActivate={onOpen}>
        <Idle icon={<TimerIcon strokeWidth={1.6} />}>Timer not running</Idle>
      </WidgetFrame>
    );
  }
  return (
    <WidgetFrame size="small" label="Timer" accent={t.accent} live={t.running} ground="deep" onActivate={onOpen}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <Ring
          value={t.value}
          label="Timer"
          valueText={`${t.text}, ${t.word}`}
          size={56}
          stroke={9}
          motion="drain"
        />
        <div>
          <Numeral text={t.text} size="s" className="text-widget-acc-ink" />
          <div className="text-notch-w-title font-medium text-widget-ink-2">{t.word}</div>
        </div>
      </div>
    </WidgetFrame>
  );
}

function BatteryWidget({ onOpen }: { onOpen: () => void }) {
  const battery = usePoll(() => commands.macBattery(), BATTERY_POLL_MS);
  return <BatteryTile battery={battery} onOpen={onOpen} />;
}

export function BatteryTile({
  battery,
  onOpen,
}: {
  /** `null` is a Mac with no battery; `undefined` is not read yet. */
  battery: Battery | null | undefined;
  onOpen?: () => void;
}) {
  if (!battery) {
    return (
      <WidgetFrame size="small" label="Battery" ground="vibrant" onActivate={onOpen}>
        <Idle icon={<BatteryIcon strokeWidth={1.6} />}>
          {battery === null ? "No battery in this Mac" : "Reading the battery"}
        </Idle>
      </WidgetFrame>
    );
  }
  const b = batteryWidget(battery);
  const Icon = battery.state === "charging" ? BatteryChargingIcon : BatteryIcon;
  return (
    <WidgetFrame size="small" label="Battery" accent={b.accent} ground="vibrant" onActivate={onOpen}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <div className="flex items-start justify-between gap-2">
          <Ring value={b.value} label="Battery charge" valueText={`${b.text}%, ${b.word}`} size={56} stroke={9}>
            <Icon aria-hidden className="size-5 text-widget-ink" strokeWidth={2} />
          </Ring>
          <span className="pt-1 text-right text-notch-w-meta leading-tight font-medium text-widget-ink-2">
            {b.word}
          </span>
        </div>
        <div className="flex items-baseline">
          <Numeral text={b.text} size="l" roll />
          <span className="text-notch-w-numeral-s font-light text-widget-ink">%</span>
        </div>
      </div>
    </WidgetFrame>
  );
}
