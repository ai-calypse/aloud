import { useEffect, useRef, useState } from "react";
import {
  FastForwardIcon,
  GlobeIcon,
  MusicIcon,
  PauseIcon,
  PlayIcon,
  RewindIcon,
  SkipBackIcon,
  SkipForwardIcon,
} from "lucide-react";
import { commands, type BrowserIssue, type NowPlaying, type Reading, type SoundState } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import { Capsule, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useNotch } from "../../context";
import {
  formatSpeed,
  formatTime,
  jsOffFix,
  joinNames,
  plainHost,
  positionAt,
  shortName,
  useArtwork,
  useNowPlaying,
  useSourceIcon,
  usePolled,
} from "../../media";
import { sourceLabel, sourceRole } from "./media-format";
import { Notice, Pill } from "./parts";
import { SKIP_MS, usePlayerControls, type Controls } from "./playback";
import { RouteTile } from "./Sound";

const PLAYER_NAME = {
  spotify: "Spotify",
  music: "Music",
  browser: "the browser",
  mediaRemote: "the app",
} as const;

const NO_SOUND: SoundState = { outputs: [], inputs: [], micMuted: null };

/**
 * Media › Now Playing in the widgets look: the Music widget at its large size
 * (art, the track, where it is playing from, a scrubber and the transport),
 * with where the sound is going beside it. A web video wears the Podcasts
 * purple and swaps previous and next for the 10 second skips and the speed
 * chip, exactly as the calm view does. Real readings only: with nothing
 * playing the tile says so and says what fills it.
 */
export function WidgetMediaPlaying() {
  const { navigate } = useNotch();
  const { reading, readAt, patch } = useNowPlaying();
  const art = useArtwork(reading.now);
  const icon = useSourceIcon(reading.now?.source.app ?? null);
  const controls = usePlayerControls(reading.now, readAt, patch);
  // The same two-second read the Sound view makes: it is a plugged-in headset
  // that should show up while you watch.
  const [sound] = usePolled(() => commands.soundState(), 2000, NO_SOUND);
  return (
    <PlayingBoard
      reading={reading}
      readAt={readAt}
      art={art}
      icon={icon}
      controls={controls}
      sound={sound}
      onOpenSound={() => navigate("media", "sound")}
    />
  );
}

export function PlayingBoard({
  reading,
  readAt,
  art,
  icon,
  controls,
  sound,
  onOpenSound,
}: {
  reading: Reading;
  readAt: number;
  art: string | null;
  icon: string | null;
  controls: Controls;
  sound: SoundState;
  onOpenSound?: () => void;
}) {
  const { now, denied, browsers } = reading;
  if (denied && !now) {
    return (
      <WidgetBoard label="Now Playing">
        <WidgetFrame size="large" label="Music">
          <Denied player={denied} />
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  if (!now) {
    return (
      <WidgetBoard label="Now Playing">
        {browsers.length > 0 ? (
          <WidgetFrame size="large" label="Podcasts">
            <BrowserNotice issues={browsers} />
          </WidgetFrame>
        ) : (
          <WidgetFrame size="medium" label="Music">
            <Notice icon={<MusicIcon strokeWidth={1.6} />} title="Nothing playing">
              Play something in any app. It shows up here, and beside the camera while the notch is closed.
            </Notice>
          </WidgetFrame>
        )}
      </WidgetBoard>
    );
  }
  return (
    <WidgetBoard label="Now Playing">
      <PlayingTile now={now} readAt={readAt} art={art} icon={icon} controls={controls} />
      {browsers.length > 0 ? (
        // The fix is several sentences: it takes the whole right half rather
        // than a tile that would hide most of it behind a scroll.
        <WidgetFrame size="large" label="Browsers">
          <BrowserNotice issues={browsers} compact />
        </WidgetFrame>
      ) : (
        <RouteTile size="medium" sound={sound} onOpen={onOpenSound} />
      )}
    </WidgetBoard>
  );
}

function Denied({ player }: { player: keyof typeof PLAYER_NAME }) {
  return (
    <Notice
      icon={<MusicIcon strokeWidth={1.6} />}
      title={`Aloud can't control ${PLAYER_NAME[player]}`}
      actions={
        <Pill solid onClick={() => void commands.openAutomationSettings()}>
          Open Automation settings
        </Pill>
      }
    >
      <p className="m-0">
        macOS asks once before one app may drive another. Allow Aloud under Privacy &amp; Security › Automation, and
        the track shows up here.
      </p>
    </Notice>
  );
}

/**
 * What to do about browsers Aloud cannot read, in the words of whichever
 * problem it is. `compact` is the note beside a track playing from somewhere
 * else; otherwise it is the whole tile.
 */
export function BrowserNotice({ issues, compact }: { issues: BrowserIssue[]; compact?: boolean }) {
  const off = issues.filter((i) => i.kind === "jsOff");
  const refused = issues.filter((i) => i.kind === "notPermitted");
  const names = joinNames(issues);
  return (
    <Notice
      icon={<GlobeIcon strokeWidth={1.6} />}
      title={
        compact
          ? `Web video from ${names} is not showing here`
          : off.length
            ? `Turn on JavaScript from Apple Events in ${joinNames(off)}`
            : `Aloud can't control ${names}`
      }
      actions={
        <>
          {off.map((i) => (
            <Pill key={i.app} solid onClick={() => void commands.mediaOpenBrowser(i.app)}>
              Open {shortName(i.app)}
            </Pill>
          ))}
          {refused.length > 0 && (
            <Pill solid onClick={() => void commands.openAutomationSettings()}>
              Open Automation settings
            </Pill>
          )}
          <Pill onClick={() => void commands.mediaRecheckBrowsers()}>Check again</Pill>
        </>
      }
    >
      {!compact && off.length > 0 && (
        <p className="m-0">
          That is how Aloud sees video playing in a tab. It reads only the title, the site and the position, nothing
          else on the page, and the switch is in the browser&apos;s menu bar:
        </p>
      )}
      {off.map((i) => (
        <p key={i.app} className="m-0">
          {jsOffFix(i.app)}
        </p>
      ))}
      {refused.length > 0 && (
        <p className="m-0">
          macOS has not allowed Aloud to control {joinNames(refused)}. Allow it under Privacy &amp; Security ›
          Automation.
        </p>
      )}
    </Notice>
  );
}

/** A transport button: a round hit target, the glyph filled as Apple draws it. */
function Round({ label, solid, onClick, children }: { label: string; solid?: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        NOTCH_FOCUS,
        "grid shrink-0 place-items-center rounded-full transition-opacity duration-[var(--dur-fast)] ease-[var(--ease-out)] active:opacity-70",
        "[&_svg]:fill-current",
        solid ? "size-12 bg-widget-ink text-widget-on [&_svg]:size-5" : "size-11 text-widget-ink [&_svg]:size-5",
      )}
    >
      {children}
    </button>
  );
}

/**
 * The large Music widget. Art with the track beside it; under that the
 * source badge, and in the lower half what the Recently Played list is in
 * Apple's: here the scrubber and the buttons that drive what is playing.
 */
export function PlayingTile({
  now,
  readAt,
  art,
  icon,
  controls,
}: {
  now: NowPlaying;
  readAt: number;
  art: string | null;
  icon: string | null;
  controls: Controls;
}) {
  const playing = now.state === "playing";
  const web = now.player === "browser";
  const role = sourceRole(now);
  const meta = role === "podcast" ? "text-widget-podcast-meta" : "text-widget-media-meta";
  return (
    <WidgetFrame size="large" label={sourceLabel(now)} accent={role} ground="accent">
      <div className="flex h-full flex-col">
        <div className="flex gap-3.5 p-3.5">
          {art ? (
            <img src={art} alt="" draggable={false} className="size-32 shrink-0 rounded-notch-row object-cover" />
          ) : (
            <span
              aria-hidden
              className="grid size-32 shrink-0 place-items-center rounded-notch-row bg-widget-tile text-widget-ink-2"
            >
              {web ? (
                <GlobeIcon className="size-8" strokeWidth={1.4} />
              ) : (
                <MusicIcon className="size-8" strokeWidth={1.4} />
              )}
            </span>
          )}
          <div className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
            <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink/80 uppercase">
              {playing ? "Now playing" : "Paused"}
            </span>
            <span className="line-clamp-2 text-notch-w-headline leading-tight font-semibold">{now.title}</span>
            <span className={cn("truncate text-notch-w-body leading-tight", meta)}>{now.artist}</span>
            <span className="truncate text-notch-w-meta leading-tight text-widget-ink/85">
              {web ? plainHost(now.source.host) : now.album}
            </span>
            <span className="mt-1.5 flex h-6 w-fit max-w-full items-center gap-1.5 rounded-full bg-widget-ink/15 pr-2.5 pl-1.5 text-notch-w-meta font-semibold">
              <AppIcon icon={icon} label="" size="sm" />
              <span className="truncate">
                {web || now.player === "mediaRemote" ? shortName(now.source.app) : now.source.app}
              </span>
            </span>
          </div>
        </div>

        {/* The lower band, where Apple's widget lists what was played: the scrubber and the transport. */}
        <div className="flex flex-1 flex-col justify-center gap-4 bg-widget-tile/30 p-3.5">
          <Scrubber now={now} readAt={readAt} onSeek={controls.seek} />
          <div className="flex items-center justify-center gap-2">
            <Round
              label={web ? "Back 10 seconds" : "Previous track"}
              onClick={() => (web ? controls.jump(-SKIP_MS) : controls.previous())}
            >
              {web ? <RewindIcon /> : <SkipBackIcon />}
            </Round>
            <Round solid label={playing ? "Pause" : "Play"} onClick={controls.toggle}>
              {playing ? <PauseIcon /> : <PlayIcon />}
            </Round>
            <Round
              label={web ? "Forward 10 seconds" : "Next track"}
              onClick={() => (web ? controls.jump(SKIP_MS) : controls.next())}
            >
              {web ? <FastForwardIcon /> : <SkipForwardIcon />}
            </Round>
            {web && (
              <Pill
                aria-label={`Playback speed ${formatSpeed(now.speedPct)}, change`}
                className="tabular-nums"
                onClick={controls.speed}
              >
                {formatSpeed(now.speedPct)}
              </Pill>
            )}
          </div>
        </div>
      </div>
    </WidgetFrame>
  );
}

/** How long a drag is believed over the player's own position. */
const HOLD_MS = 900;
/** Quarter-second ticks: at 310pt a 3-minute track moves under a point a tick. */
const TICK_MS = 250;

/**
 * The scrubber: a capsule with the elapsed time and the length under it. The
 * clock runs here between Rust's one-second polls. While the thumb is held its
 * value rules (a poll landing mid-drag would yank it back), and the seek is
 * sent once the drag pauses rather than once per step.
 */
function Scrubber({ now, readAt, onSeek }: { now: NowPlaying; readAt: number; onSeek: (ms: number) => void }) {
  const [, tick] = useState(0);
  const held = useRef<{ ms: number; until: number } | null>(null);
  const seekTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const playing = now.state === "playing";
  const seekable = now.durationMs > 0;

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(() => tick((n) => n + 1), TICK_MS);
    return () => clearInterval(timer);
  }, [playing]);
  useEffect(() => () => clearTimeout(seekTimer.current), []);

  const at = performance.now();
  const hold = held.current;
  const position = hold && at < hold.until ? hold.ms : positionAt(now, readAt, at);
  const text = `${formatTime(position)} of ${formatTime(now.durationMs)}`;

  return (
    <div className="flex flex-col gap-1">
      <Capsule
        onFill
        label="Position"
        valueText={text}
        value={seekable ? position / now.durationMs : 0}
        className="h-2.5"
        // Nothing to scrub on a live stream; the range input would only lie.
        onChange={
          seekable
            ? (v) => {
                const ms = Math.round(v * now.durationMs);
                held.current = { ms, until: performance.now() + HOLD_MS };
                tick((n) => n + 1);
                clearTimeout(seekTimer.current);
                seekTimer.current = setTimeout(() => onSeek(ms), 150);
              }
            : undefined
        }
      />
      <div className="flex justify-between text-notch-w-meta leading-none font-medium tabular-nums text-widget-ink/85">
        <span>{formatTime(position)}</span>
        <span>{formatTime(now.durationMs)}</span>
      </div>
    </div>
  );
}
