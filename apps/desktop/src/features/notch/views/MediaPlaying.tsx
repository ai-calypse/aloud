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
import { commands, type BrowserIssue, type NowPlaying, type Transport } from "@/lib/bindings";
import { AppIcon } from "@/components/notch/app-icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/notch/controls";
import { Empty, Lead, Value } from "@/components/notch/list";
import {
  formatSpeed,
  formatTime,
  jsOffFix,
  joinNames,
  nextSpeed,
  plainHost,
  positionAt,
  shortName,
  useArtwork,
  useNowPlaying,
  useSourceIcon,
} from "../media";

const PLAYER_NAME = { spotify: "Spotify", music: "Music", browser: "the browser" } as const;

/** What the transport buttons send. Errors surface as the next reading not changing. */
function send(now: NowPlaying, transport: Transport) {
  try {
    void commands.mediaControl(now.player, transport);
  } catch {
    /* browser preview */
  }
}

/** How far the skip buttons jump a web video, which has no previous or next. */
const SKIP_MS = 10_000;

/**
 * What to do about browsers Aloud cannot read, in the words of whichever
 * problem it is. `compact` is the line under a track that is playing from
 * somewhere else; otherwise it is the whole view.
 */
function BrowserIssues({ issues, compact }: { issues: BrowserIssue[]; compact?: boolean }) {
  const off = issues.filter((i) => i.kind === "jsOff");
  const refused = issues.filter((i) => i.kind === "notPermitted");
  const recheck = () => void commands.mediaRecheckBrowsers();
  const buttons = (
    <>
      {off.map((i) => (
        <Button
          key={i.app}
          variant="notch"
          size="notch"
          onClick={() => void commands.mediaOpenBrowser(i.app)}
        >
          Open {shortName(i.app)}
        </Button>
      ))}
      {refused.length > 0 && (
        <Button variant="notch" size="notch" onClick={() => void commands.openAutomationSettings()}>
          Open Automation settings
        </Button>
      )}
      <Button variant="notch-quiet" size="notch" onClick={recheck}>
        Check again
      </Button>
    </>
  );
  const names = joinNames(issues);
  const body = (
    <>
      {off.map((i) => (
        <span key={i.app} className="block">
          {jsOffFix(i.app)}
        </span>
      ))}
      {refused.length > 0 && (
        <span className="block">
          macOS has not allowed Aloud to control {joinNames(refused)}. Allow it under Privacy &amp;
          Security › Automation.
        </span>
      )}
    </>
  );
  if (compact) {
    return (
      <div className="flex flex-col gap-1.5 border-t border-notch-edge pt-3 font-notch text-notch-meta text-notch-ink-3">
        <p className="m-0 text-pretty">
          Web video from {names} is not showing here. {body}
        </p>
        <div className="flex items-center gap-1">{buttons}</div>
      </div>
    );
  }
  return (
    <Empty
      title={
        off.length
          ? `Turn on JavaScript from Apple Events in ${joinNames(off)}`
          : `Aloud can't control ${names}`
      }
      action={<div className="flex items-center gap-1">{buttons}</div>}
    >
      {off.length > 0 && (
        <>
          That is how Aloud sees video playing in a tab. It reads only the title, the site and the
          position, nothing else on the page, and the switch is in the browser&apos;s menu bar:
        </>
      )}
      {body}
    </Empty>
  );
}

/**
 * Media › Now Playing: what Spotify, Music or a browser tab is playing, with
 * the controls. The closed notch shows the same track as an island; this is it
 * one step deeper (scrub, previous and next, or for a web video skip and speed).
 */
export function MediaPlaying() {
  const { reading, readAt, patch } = useNowPlaying();
  const { now, denied, browsers } = reading;
  const art = useArtwork(now);
  const icon = useSourceIcon(now?.source.app ?? null);

  if (denied && !now) {
    return (
      <Empty
        title={`Aloud can't control ${PLAYER_NAME[denied]}`}
        action={
          <Button
            variant="notch"
            size="notch"
            onClick={() => void commands.openAutomationSettings()}
          >
            Open Automation settings
          </Button>
        }
      >
        macOS asks once before one app may drive another. Allow Aloud under Privacy &amp; Security ›
        Automation, and the track shows up here.
      </Empty>
    );
  }
  if (!now) {
    if (browsers.length) return <BrowserIssues issues={browsers} />;
    return (
      <Empty title="Nothing playing">
        Play something in Spotify, Music or a browser tab. It shows up here, and beside the camera
        while the notch is closed.
      </Empty>
    );
  }

  const playing = now.state === "playing";
  const web = now.player === "browser";
  const jump = (by: number) => {
    const to = Math.max(
      0,
      Math.min(now.durationMs || Infinity, positionAt(now, readAt, performance.now()) + by),
    );
    patch((n) => ({ ...n, positionMs: to }));
    try {
      void commands.mediaSeek(now.player, to);
    } catch {
      /* browser preview */
    }
  };
  return (
    <div className="flex flex-col gap-3 font-notch">
      <div className="flex items-center gap-4">
        {art ? (
          <img
            src={art}
            alt=""
            draggable={false}
            className="size-[104px] shrink-0 rounded-notch-row object-cover"
          />
        ) : (
          <span
            aria-hidden
            className="grid size-[104px] shrink-0 place-items-center rounded-notch-row bg-notch-well text-notch-ink-3"
          >
            {web ? (
              <GlobeIcon className="size-6" strokeWidth={1.6} />
            ) : (
              <MusicIcon className="size-6" strokeWidth={1.6} />
            )}
          </span>
        )}
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          <Lead
            sub={
              <span className="block truncate">
                {[now.artist, web ? plainHost(now.source.host) : now.album]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            }
          >
            <span className="block truncate">{now.title}</span>
          </Lead>
          <Progress
            now={now}
            readAt={readAt}
            onSeek={(ms) => {
              patch((n) => ({ ...n, positionMs: ms }));
              try {
                void commands.mediaSeek(now.player, ms);
              } catch {
                /* browser preview */
              }
            }}
          />
          <div className="flex items-center gap-1">
            <Button
              variant="notch-quiet"
              size="notch-icon"
              aria-label={web ? "Back 10 seconds" : "Previous track"}
              onClick={() => (web ? jump(-SKIP_MS) : send(now, "previous"))}
            >
              {web ? <RewindIcon /> : <SkipBackIcon />}
            </Button>
            <Button
              variant="notch"
              size="notch-icon"
              aria-label={playing ? "Pause" : "Play"}
              onClick={() => {
                patch((n) => ({ ...n, state: playing ? "paused" : "playing" }));
                send(now, "toggle");
              }}
            >
              {playing ? <PauseIcon /> : <PlayIcon />}
            </Button>
            <Button
              variant="notch-quiet"
              size="notch-icon"
              aria-label={web ? "Forward 10 seconds" : "Next track"}
              onClick={() => (web ? jump(SKIP_MS) : send(now, "next"))}
            >
              {web ? <FastForwardIcon /> : <SkipForwardIcon />}
            </Button>
            {web && (
              <Button
                variant="notch-quiet"
                size="notch"
                aria-label={`Playback speed ${formatSpeed(now.speedPct)}, change`}
                className="tabular-nums"
                onClick={() => {
                  const to = nextSpeed(now.speedPct);
                  patch((n) => ({ ...n, speedPct: to }));
                  try {
                    void commands.mediaSetSpeed(now.player, to);
                  } catch {
                    /* browser preview */
                  }
                }}
              >
                {formatSpeed(now.speedPct)}
              </Button>
            )}
            <Badge variant="notch" className="ml-auto gap-1.5">
              <AppIcon icon={icon} label="" size="sm" className="size-3.5 rounded-[4px]" />
              {web ? shortName(now.source.app) : now.source.app}
            </Badge>
          </div>
        </div>
      </div>
      {browsers.length > 0 && <BrowserIssues issues={browsers} compact />}
    </div>
  );
}

/** How long a drag is believed over the player's own position. */
const HOLD_MS = 900;
/** Quarter-second ticks: at 380px a 3-minute track moves half a pixel a tick. */
const TICK_MS = 250;

/**
 * The scrubber: elapsed, a slider, the length. The clock runs here between
 * Rust's one-second polls. While the thumb is being dragged its value rules
 * (a poll landing mid-drag would yank it back), and the seek is sent once the
 * drag pauses rather than once per pixel.
 */
function Progress({
  now,
  readAt,
  onSeek,
}: {
  now: NowPlaying;
  readAt: number;
  onSeek: (ms: number) => void;
}) {
  const [, tick] = useState(0);
  const held = useRef<{ ms: number; until: number } | null>(null);
  const seekTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const playing = now.state === "playing";

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(() => tick((n) => n + 1), TICK_MS);
    return () => clearInterval(timer);
  }, [playing]);
  useEffect(() => () => clearTimeout(seekTimer.current), []);

  const at = performance.now();
  const hold = held.current;
  const position = hold && at < hold.until ? hold.ms : positionAt(now, readAt, at);

  return (
    <div className="flex items-center gap-2.5">
      <Value className="w-10 text-left">{formatTime(position)}</Value>
      <Slider
        className="flex-1"
        label="Position"
        min={0}
        max={Math.max(1, now.durationMs)}
        step={500}
        value={Math.min(position, Math.max(1, now.durationMs))}
        disabled={now.durationMs <= 0}
        onChange={(ms) => {
          held.current = { ms, until: performance.now() + HOLD_MS };
          tick((n) => n + 1);
          clearTimeout(seekTimer.current);
          seekTimer.current = setTimeout(() => onSeek(ms), 150);
        }}
      />
      <Value className="w-10 text-right">{formatTime(now.durationMs)}</Value>
    </div>
  );
}
