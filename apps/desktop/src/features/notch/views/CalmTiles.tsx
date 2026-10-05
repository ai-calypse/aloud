import type { ReactNode } from "react";
import {
  BatteryChargingIcon,
  BatteryIcon,
  BotIcon,
  CalendarIcon,
  CheckIcon,
  CloudIcon,
  MicIcon,
  MoonIcon,
  MusicIcon,
  NotebookPenIcon,
  PauseIcon,
  PlayIcon,
  SkipBackIcon,
  SkipForwardIcon,
  SunIcon,
  TimerIcon,
  Volume2Icon,
  XIcon,
  ZapIcon,
} from "lucide-react";
import { Capsule, Numeral, Ring, SegBar, Spark } from "@/components/notch/widgets";
import { Tile, TileLabel, type TilePlacement } from "@/components/notch/tile";
import { NOTCH_FOCUS } from "@/components/ui/button";
import type { Battery, FocusSnapshot, Island, NowPlaying, Weather } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import {
  calmAgents,
  calmBattery,
  calmCalendar,
  calmMedia,
  calmTimer,
  calmUsage,
  calmWeather,
  type AgentRow,
} from "./calm-home";
import { SKY } from "./sky";

/**
 * The eight Calm tiles. Each is pure of data fetching: it takes what it says
 * and draws the layout its `placement.size` asks for (the prototype's xL, xM
 * and xS), so the tune page can show any state without Tauri and CalmHome is
 * only wiring. A tile whose source is off says so in a line; it never draws
 * a made-up value.
 */

const NUMERAL = {
  hero: "text-notch-n-hero",
  xl: "text-notch-n-xl",
  lg: "text-notch-n-lg",
  md: "text-notch-n-md",
  sm: "text-notch-n-sm",
} as const;

/** The Light numeral at one of Calm's five sizes, tracked tight. */
function Num({
  text,
  size,
  roll,
  className,
}: {
  text: string;
  size: keyof typeof NUMERAL;
  roll?: boolean;
  className?: string;
}) {
  return (
    <Numeral
      text={text}
      tracking="calm"
      roll={roll}
      className={cn(NUMERAL[size], "text-notch-ink", className)}
    />
  );
}

/** What a tile with nothing behind it says: its name, then one honest line. */
function Idle({
  icon,
  label,
  title,
  hint,
}: {
  icon: ReactNode;
  label: string;
  title: string;
  hint?: string;
}) {
  return (
    <>
      <TileLabel icon={icon}>{label}</TileLabel>
      <div className="mt-auto min-w-0">
        <div className="text-notch-body leading-tight font-medium text-notch-ink-2">{title}</div>
        {hint && (
          <div className="mt-0.5 line-clamp-2 text-notch-meta leading-snug text-notch-ink-3">{hint}</div>
        )}
      </div>
    </>
  );
}

/** A small word-bearing chip; with a state it is that accent, else a quiet well. */
function Chip({ state, children }: { state?: "ok" | "caution" | "alert"; children: ReactNode }) {
  return (
    <span
      data-tile-state={state}
      className={cn(
        "inline-flex h-5.5 w-fit items-center rounded-full px-2.25 text-notch-meta font-medium whitespace-nowrap",
        state ? "bg-notch-acc/18 font-semibold text-notch-acc-ink" : "bg-notch-field text-notch-ink-3",
      )}
    >
      {children}
    </span>
  );
}

function Round({
  label,
  onClick,
  solid,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  solid?: boolean;
  className: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        NOTCH_FOCUS,
        "grid shrink-0 place-items-center rounded-full transition-[scale,background-color] duration-[var(--dur-fast)] ease-[var(--ease-out)] active:scale-92",
        solid
          ? "bg-notch-ink text-notch-on hover:bg-notch-ink/90"
          : "bg-notch-field text-notch-ink hover:bg-notch-raised",
        className,
      )}
    >
      {children}
    </button>
  );
}

/* ---------------------------------------------------------------------------
 * Now playing
 * ------------------------------------------------------------------------- */

function Art({ src, className }: { src: string | null; className: string }) {
  return src ? (
    <img src={src} alt="" draggable={false} className={cn("shrink-0 object-cover", className)} />
  ) : (
    <span
      aria-hidden
      className={cn("grid shrink-0 place-items-center bg-notch-field text-notch-ink-3", className)}
    >
      <MusicIcon className="size-1/3" strokeWidth={1.5} />
    </span>
  );
}

function Bars({ playing, count, height }: { playing: boolean; count: number; height: number }) {
  return (
    <span aria-hidden className="notch-bars" style={{ height }} data-playing={playing ? "" : undefined}>
      {Array.from({ length: count }, (_, i) => (
        <i key={i} />
      ))}
    </span>
  );
}

export function MediaTile({
  placement,
  track,
  art,
  positionMs,
  onToggle,
  onPrevious,
  onNext,
  onSeek,
  onOpen,
}: {
  placement: TilePlacement;
  track: NowPlaying | null;
  art: string | null;
  positionMs: number;
  onToggle: () => void;
  onPrevious: () => void;
  onNext: () => void;
  /** 0…1 of the track. */
  onSeek: (fraction: number) => void;
  onOpen?: () => void;
}) {
  const label = "Now playing";
  if (!track) {
    return (
      <Tile placement={placement} label={label} onActivate={onOpen}>
        <Idle
          icon={<MusicIcon />}
          label={label}
          title="Nothing playing"
          hint={placement.size === "L" ? "Play something in any app and it shows up here." : undefined}
        />
      </Tile>
    );
  }
  const m = calmMedia(track, positionMs);
  const toggle = (
    <Round
      label={m.playing ? "Pause" : "Play"}
      onClick={onToggle}
      solid
      className={placement.size === "L" ? "size-11.5" : "size-8.5"}
    >
      {m.playing ? (
        <PauseIcon className="size-4.5" fill="currentColor" />
      ) : (
        <PlayIcon className="size-4.5" fill="currentColor" />
      )}
    </Round>
  );
  if (placement.size === "L") {
    return (
      <Tile placement={placement} label={label} onActivate={onOpen}>
        <div className="flex gap-3">
          <Art src={art} className="size-22 rounded-2xl" />
          <div className="grid min-w-0 flex-1 content-start gap-0.5 pt-0.5">
            <TileLabel icon={<Volume2Icon />}>{m.playing ? label : "Paused"}</TileLabel>
            <div className="truncate text-notch-tile-title leading-tight font-semibold tracking-tight">
              {m.title}
            </div>
            <div className="truncate text-notch-meta text-notch-ink-3">
              {[m.artist, track.album].filter(Boolean).join(" · ")}
            </div>
            <div className="mt-1">
              <Bars playing={m.playing} count={5} height={18} />
            </div>
          </div>
        </div>
        <div className="mt-auto flex items-center gap-2 text-notch-label text-notch-ink-3 tabular-nums">
          <span className="min-w-9">{m.position}</span>
          {m.scrub ? (
            <Capsule
              value={m.progress}
              label="Track position"
              valueText={`${m.position} of ${formatLength(track.durationMs)}`}
              onChange={onSeek}
              onFill
              className="h-2 flex-1"
            />
          ) : (
            <span className="flex-1" />
          )}
          <span className="min-w-9 text-right">{m.left}</span>
        </div>
        <div className="mt-2.5 flex items-center justify-center gap-3">
          <Round label="Previous" onClick={onPrevious} className="size-9.5">
            <SkipBackIcon className="size-4.5" fill="currentColor" />
          </Round>
          {toggle}
          <Round label="Next" onClick={onNext} className="size-9.5">
            <SkipForwardIcon className="size-4.5" fill="currentColor" />
          </Round>
        </div>
      </Tile>
    );
  }
  return (
    <Tile placement={placement} label={label} onActivate={onOpen}>
      <div className="flex h-full items-center gap-2.5">
        <Art src={art} className="size-16 rounded-xl" />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <div className="truncate text-notch-tile-name leading-tight font-semibold tracking-tight">
            {m.title}
          </div>
          <div className="truncate text-notch-meta text-notch-ink-3">{m.artist}</div>
          <div className="mt-0.5">
            <Bars playing={m.playing} count={4} height={12} />
          </div>
        </div>
        {toggle}
      </div>
    </Tile>
  );
}

function formatLength(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/* ---------------------------------------------------------------------------
 * Focus timer
 * ------------------------------------------------------------------------- */

export function TimerTile({
  placement,
  snap,
  now,
  onAction,
  onOpen,
}: {
  placement: TilePlacement;
  snap: FocusSnapshot | null;
  now: number;
  /** An action id from the notch's registry: `focus.pause`, `focus.resume`. */
  onAction: (id: string) => void;
  onOpen?: () => void;
}) {
  const c = calmTimer(snap, now);
  if (c.idle) {
    return (
      <Tile placement={placement} label="Timer" onActivate={onOpen}>
        <Idle
          icon={<TimerIcon />}
          label="Timer"
          title="Not running"
          hint={placement.size === "L" ? "Start a focus session from Focus." : undefined}
        />
      </Tile>
    );
  }
  const ring = (size: number, stroke: number, inner: ReactNode) => (
    <Ring
      value={c.value}
      label="Timer"
      valueText={`${c.clock}, ${c.word}`}
      size={size}
      stroke={stroke}
      motion="drain"
      className="shrink-0"
    >
      {inner}
    </Ring>
  );
  if (placement.size === "L") {
    return (
      <Tile placement={placement} label="Timer" onActivate={onOpen}>
        <div className="flex items-center justify-between">
          <TileLabel icon={<TimerIcon />}>{c.word}</TileLabel>
          {c.chip && <Chip>{c.chip}</Chip>}
        </div>
        <div className="mt-2 grid place-items-center">
          {ring(
            126,
            15,
            <span className="grid justify-items-center gap-0.5">
              <Num text={c.clock} size="lg" roll />
              <span className="text-notch-label font-medium text-notch-ink-3">
                {snap?.timer.mode === "pomodoro" ? "left" : "elapsed"}
              </span>
            </span>,
          )}
        </div>
        <div className="mt-auto flex items-center gap-2.5">
          {c.rounds && (
            <SegBar
              value={c.rounds.done / c.rounds.of}
              segments={c.rounds.of}
              label="Sessions"
              valueText={`${c.rounds.done} of ${c.rounds.of}`}
              height={6}
              gap={3}
              className="flex-1"
            />
          )}
          {c.rounds ? null : <span className="flex-1" />}
          {c.button && (
            <button
              type="button"
              onClick={() => onAction(c.button!.action)}
              className={cn(
                NOTCH_FOCUS,
                "h-7 rounded-full bg-notch-field px-3 text-notch-body font-medium text-notch-ink transition-[scale,background-color] duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:bg-notch-raised active:scale-96",
              )}
            >
              {c.button.label}
            </button>
          )}
        </div>
      </Tile>
    );
  }
  if (placement.size === "M") {
    return (
      <Tile placement={placement} label="Timer" onActivate={onOpen}>
        <div className="flex h-full items-center gap-3">
          {ring(70, 9, null)}
          <div className="grid min-w-0 gap-0.5">
            <TileLabel>{[c.word, c.chip].filter(Boolean).join(" · ")}</TileLabel>
            <Num text={c.clock} size="lg" roll />
          </div>
        </div>
      </Tile>
    );
  }
  return (
    <Tile placement={placement} label="Timer" onActivate={onOpen}>
      <div className="flex h-full flex-col items-center justify-center gap-1.5">
        {ring(60, 8, <Num text={c.minutes} size="md" roll />)}
        <TileLabel className="justify-center">min · {c.word}</TileLabel>
      </div>
    </Tile>
  );
}

/* ---------------------------------------------------------------------------
 * Agents
 * ------------------------------------------------------------------------- */

function Glyph({ state }: { state: AgentRow["state"] }) {
  switch (state) {
    case "wait":
      return (
        <span className="grid size-3.5 place-items-center rounded-full bg-notch-caution">
          <span className="mt-px h-1.5 w-0.5 rounded-sm bg-notch-on" />
        </span>
      );
    case "run":
      return (
        <span className="size-4 animate-spin rounded-full border-[2.5px] border-notch-raised border-t-notch-ink" />
      );
    case "done":
      return <CheckIcon className="size-4 text-notch-ok" strokeWidth={2.5} />;
    case "fail":
      return <XIcon className="size-4 text-notch-danger" strokeWidth={2.5} />;
    case "idle":
      return <span className="size-2 rounded-full bg-notch-raised" />;
  }
}

export function AgentsTile({
  placement,
  sessions,
  permissions,
  voice,
  names,
  quota,
  onOpen,
}: {
  placement: TilePlacement;
  sessions: Parameters<typeof calmAgents>[0];
  permissions: Parameters<typeof calmAgents>[1];
  voice: Parameters<typeof calmAgents>[2];
  names: Record<string, string>;
  quota: Parameters<typeof calmUsage>[0];
  onOpen?: () => void;
}) {
  const a = calmAgents(sessions, permissions, voice, names);
  if (a.rows.length === 0) {
    return (
      <Tile placement={placement} label="Agents" onActivate={onOpen}>
        <Idle
          icon={<BotIcon />}
          label="Agents"
          title="No agents running"
          hint="Claude Code and Codex show up here while they work."
        />
      </Tile>
    );
  }
  const chip = <Chip state={a.needs > 0 ? "caution" : undefined}>{a.word}</Chip>;
  if (placement.size === "L") {
    const usage = calmUsage(quota);
    return (
      <Tile placement={placement} label="Agents" onActivate={onOpen}>
        <div className="flex items-center justify-between">
          <TileLabel icon={<BotIcon />}>Agents</TileLabel>
          {chip}
        </div>
        <ul className="m-0 mt-1.5 grid list-none gap-0.5 p-0">
          {a.rows.slice(0, 3).map((row) => (
            <li key={row.id} className="flex h-9 min-w-0 items-center gap-2">
              <span className="grid size-4.5 shrink-0 place-items-center">
                <Glyph state={row.state} />
              </span>
              <div className="min-w-0 flex-1 leading-tight">
                <div className="truncate text-notch-body font-medium">{row.name}</div>
                <div className="truncate text-notch-label text-notch-ink-3">{row.detail}</div>
              </div>
            </li>
          ))}
        </ul>
        {usage && (
          <div data-tile-state={usage.state} className="mt-auto grid gap-1.5">
            <div className="flex justify-between text-notch-label font-medium text-notch-ink-3">
              <span>Plan usage</span>
              <b className="font-semibold text-notch-acc-ink">
                {usage.percent}%{usage.word && ` · ${usage.word}`}
              </b>
            </div>
            <SegBar
              value={usage.percent / 100}
              segments={10}
              label="Plan usage"
              valueText={`${usage.percent}%${usage.word ? `, ${usage.word}` : ""}`}
              height={7}
              gap={3}
            />
          </div>
        )}
      </Tile>
    );
  }
  return (
    <Tile placement={placement} label="Agents" onActivate={onOpen}>
      <div className="flex h-full items-center gap-3">
        <Num text={String(a.active)} size="xl" roll />
        <div className="grid min-w-0 gap-1.5">
          <TileLabel icon={<BotIcon />}>Agents</TileLabel>
          {chip}
        </div>
      </div>
    </Tile>
  );
}

/* ---------------------------------------------------------------------------
 * Battery
 * ------------------------------------------------------------------------- */

export function BatteryTile({
  placement,
  battery,
  onOpen,
}: {
  placement: TilePlacement;
  /** `null` is a Mac with no battery; `undefined` is not read yet. */
  battery: Battery | null | undefined;
  onOpen?: () => void;
}) {
  if (!battery) {
    return (
      <Tile placement={placement} label="Battery" onActivate={onOpen}>
        <Idle
          icon={<BatteryIcon />}
          label="Battery"
          title={battery === null ? "No battery" : "Reading…"}
          hint={placement.size === "L" && battery === null ? "This Mac runs on power." : undefined}
        />
      </Tile>
    );
  }
  const b = calmBattery(battery);
  const word = (
    <div className="flex items-center gap-1 text-notch-label font-semibold whitespace-nowrap text-notch-acc-ink">
      {b.charging && <ZapIcon aria-hidden className="size-3" fill="currentColor" />}
      {b.word}
    </div>
  );
  const label = (
    <TileLabel icon={b.charging ? <BatteryChargingIcon /> : <BatteryIcon />}>Battery</TileLabel>
  );
  const meter = (vertical: boolean) => (
    <Capsule
      value={b.percent / 100}
      label="Battery charge"
      valueText={`${b.percent}%, ${b.word}`}
      vertical={vertical}
      className={vertical ? "rounded-notch-tile" : "h-2.25"}
    />
  );
  return (
    <Tile placement={placement} state={b.state} label="Battery" onActivate={onOpen}>
      {placement.size === "L" ? (
        <div className="flex h-full flex-col items-start gap-2">
          {label}
          <Num text={`${b.percent}%`} size="md" roll />
          <div className="min-h-0 w-full flex-1">{meter(true)}</div>
          {word}
        </div>
      ) : (
        <div className="flex h-full flex-col justify-between gap-1">
          {label}
          <Num text={`${b.percent}%`} size="lg" roll />
          {meter(false)}
          {word}
        </div>
      )}
    </Tile>
  );
}

/* ---------------------------------------------------------------------------
 * Calendar
 * ------------------------------------------------------------------------- */

export function CalendarTile({
  placement,
  islands,
  now,
  onOpen,
}: {
  placement: TilePlacement;
  islands: Island[];
  now: number;
  onOpen?: () => void;
}) {
  const c = calmCalendar(islands, now);
  const next = c.next;
  const soon = c.minutes === null ? c.next?.when : c.minutes === 0 ? "Now" : String(c.minutes);

  if (placement.size === "L") {
    return (
      <Tile placement={placement} label="Calendar" onActivate={onOpen}>
        <div className="grid h-full min-h-0 grid-cols-[1fr_156px] gap-4">
          <div className="flex min-h-0 flex-col gap-1.5">
            <div>
              <div className="text-notch-meta font-semibold tracking-wide text-notch-ink-3">
                {c.weekday.toUpperCase()}
              </div>
              <Num text={c.day} size="hero" />
            </div>
            {next ? (
              <div className="grid grid-cols-[3px_1fr] gap-2 rounded-[10px] bg-notch-field py-1.5 pr-2.5 pl-2 leading-tight">
                <span className="rounded-sm bg-notch-ink" />
                <div className="grid min-w-0">
                  <b className="truncate text-notch-body font-semibold">{next.title}</b>
                  <span className="text-notch-label text-notch-ink-3">{next.when}</span>
                </div>
              </div>
            ) : (
              <p className="m-0 flex items-center gap-1.5 text-notch-meta text-notch-ink-3">
                <CalendarIcon aria-hidden className="size-3.5 shrink-0" />
                Nothing in the next hour
              </p>
            )}
          </div>
          <div className="flex flex-col">
            <div className="mb-1.5 text-right text-notch-meta font-semibold tracking-wide text-notch-ink-3">
              {c.month.month.toUpperCase()}
            </div>
            <div className="grid grid-cols-7 content-start gap-y-px text-center text-notch-tile-label tabular-nums">
              {c.month.initials.map((d, i) => (
                <b key={i} className="pb-0.5 text-notch-micro font-medium text-notch-ink-3">
                  {d}
                </b>
              ))}
              {c.month.cells.map((cell, i) =>
                cell ? (
                  <span
                    key={i}
                    className={cn(
                      "grid h-5 place-items-center font-medium",
                      cell.weekend && "text-notch-ink-3",
                      cell.today && "size-5 justify-self-center rounded-full bg-notch-ink font-bold text-notch-on",
                    )}
                  >
                    {cell.day}
                  </span>
                ) : (
                  <span key={i} />
                ),
              )}
            </div>
          </div>
        </div>
      </Tile>
    );
  }

  if (placement.size === "M") {
    return (
      <Tile placement={placement} label="Calendar" onActivate={onOpen}>
        <TileLabel icon={<CalendarIcon />}>{next ? `Next · ${next.title}` : c.weekday}</TileLabel>
        <div className="mt-0.5 flex items-baseline gap-2.5">
          <Num text={next ? (soon ?? "") : c.day} size="xl" />
          {next && c.minutes ? <span className="text-notch-n-md font-light text-notch-ink-3">min</span> : null}
        </div>
        {next ? (
          <div className="mt-auto grid gap-1.5">
            <Capsule value={c.progress} label="Time until the event" valueText={next.when} className="h-1.5" />
          </div>
        ) : (
          <p className="m-0 mt-auto text-notch-meta text-notch-ink-3">Nothing in the next hour</p>
        )}
      </Tile>
    );
  }

  return (
    <Tile placement={placement} label="Calendar" onActivate={onOpen}>
      <TileLabel icon={<CalendarIcon />}>{next ? "Next" : "Today"}</TileLabel>
      <Num text={next ? (soon ?? "") : c.day} size="lg" className="mt-1" />
      <span className="mt-auto truncate text-notch-meta text-notch-ink-3">
        {next ? `${next.title}${c.minutes ? ` · ${c.minutes}m` : ""}` : c.weekday}
      </span>
    </Tile>
  );
}

/* ---------------------------------------------------------------------------
 * Weather
 * ------------------------------------------------------------------------- */

/** The sun and cloud keep their own colours; every other sky is a glyph in ink. */
function WeatherArt({ data, size }: { data: Weather; size: number }) {
  const style = { width: size, height: size };
  if (data.sky === "partlyCloudy" && data.isDay) {
    return (
      <svg aria-hidden viewBox="0 0 64 64" style={style} className="shrink-0">
        <circle cx="24" cy="24" r="11" className="fill-notch-sun" />
        <g strokeWidth="3" strokeLinecap="round" className="stroke-notch-sun">
          <path d="M24 5v4M24 39v4M5 24h4M39 24h4M10.5 10.5l2.8 2.8M35.5 35.5l2.8 2.8M10.5 37.5l2.8-2.8M35.5 12.5l2.8-2.8" />
        </g>
        <path d="M20 49a9 9 0 0 1 1.5-17.9A12 12 0 0 1 44.6 33 8 8 0 0 1 45 49z" className="fill-notch-cloud" />
        <path d="M20 49a9 9 0 0 0 .9-5h26.4A8 8 0 0 1 45 49z" className="fill-notch-cloud-shade" />
      </svg>
    );
  }
  const Icon = data.sky === "clear" ? (data.isDay ? SunIcon : MoonIcon) : (SKY[data.sky] ?? CloudIcon);
  return (
    <Icon
      aria-hidden
      style={style}
      strokeWidth={1.4}
      className={cn("shrink-0", data.sky === "clear" && data.isDay ? "text-notch-sun" : "text-notch-ink-2")}
    />
  );
}

export function WeatherTile({
  placement,
  data,
  loading,
  onOpen,
}: {
  placement: TilePlacement;
  data: Weather | null;
  loading: boolean;
  onOpen?: () => void;
}) {
  if (!data) {
    return (
      <Tile placement={placement} label="Weather" onActivate={onOpen}>
        <Idle
          icon={<CloudIcon />}
          label="Weather"
          title={loading ? "Checking the sky" : "Weather unavailable"}
        />
      </Tile>
    );
  }
  const w = calmWeather(data);
  const label = <TileLabel icon={<SunIcon />}>Weather</TileLabel>;
  if (placement.size === "L") {
    return (
      <Tile placement={placement} label="Weather" onActivate={onOpen}>
        {label}
        <div className="mt-1.5 flex items-start justify-between">
          <Num text={w.temp} size="hero" roll />
          <WeatherArt data={data} size={60} />
        </div>
        <div className="mt-0.5 text-notch-meta text-notch-ink-3">
          {[w.summary, w.range].filter(Boolean).join(" · ")}
        </div>
        {w.highs.length > 1 && (
          <div className="mt-auto">
            <Spark
              values={w.highs}
              label={`Daily highs, ${w.highs.length} days`}
              width={156}
              height={30}
              fill
              className="w-full"
            />
          </div>
        )}
      </Tile>
    );
  }
  if (placement.size === "M") {
    return (
      <Tile placement={placement} label="Weather" onActivate={onOpen}>
        <div className="flex h-full items-center gap-3">
          <WeatherArt data={data} size={54} />
          <div className="grid min-w-0 flex-1 gap-0.5">
            {label}
            <Num text={w.temp} size="xl" roll />
            <span className="truncate text-notch-meta text-notch-ink-3">{w.summary}</span>
          </div>
          {w.highs.length > 1 && (
            <Spark values={w.highs} label={`Daily highs, ${w.highs.length} days`} width={70} height={34} fill />
          )}
        </div>
      </Tile>
    );
  }
  return (
    <Tile placement={placement} label="Weather" onActivate={onOpen}>
      <div className="flex h-full items-center gap-2">
        <WeatherArt data={data} size={46} />
        <div className="grid min-w-0 gap-0.5">
          <Num text={w.temp} size="lg" roll />
          <span className="truncate text-notch-meta text-notch-ink-3">{w.summary}</span>
        </div>
      </div>
    </Tile>
  );
}

/* ---------------------------------------------------------------------------
 * Quick tiles
 * ------------------------------------------------------------------------- */

function QuickTile({
  placement,
  label,
  hint,
  icon,
  onActivate,
}: {
  placement: TilePlacement;
  label: string;
  hint: string;
  icon: ReactNode;
  onActivate?: () => void;
}) {
  return (
    <Tile placement={placement} label={label} onActivate={onActivate}>
      <div className="flex h-full flex-col items-start justify-between">
        <span
          aria-hidden
          className="grid size-9.5 place-items-center rounded-full bg-notch-ink text-notch-on [&_svg]:size-4.5"
        >
          {icon}
        </span>
        <div className="max-w-full text-left leading-tight">
          <b className="block text-notch-body font-semibold">{label}</b>
          <span className="block truncate text-notch-label text-notch-ink-3">{hint}</span>
        </div>
      </div>
    </Tile>
  );
}

/** Dictation is a held key, so this tile teaches the key and has nothing to press. */
export function DictateTile({ placement, hint }: { placement: TilePlacement; hint: string }) {
  return <QuickTile placement={placement} label="Dictate" hint={hint} icon={<MicIcon />} />;
}

export function NoteTile({ placement, onActivate }: { placement: TilePlacement; onActivate: () => void }) {
  return (
    <QuickTile
      placement={placement}
      label="New note"
      hint="Notes"
      icon={<NotebookPenIcon />}
      onActivate={onActivate}
    />
  );
}
