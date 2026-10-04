import { useEffect, useRef, useState } from "react";
import { MusicIcon, PauseIcon, PlayIcon, SkipBackIcon, SkipForwardIcon } from "lucide-react";
import { commands, type NowPlaying, type Transport } from "@/lib/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/notch/controls";
import { Empty, Lead, Value } from "@/components/notch/list";
import { formatTime, positionAt, useArtwork, useNowPlaying } from "../media";

const PLAYER_NAME = { spotify: "Spotify", music: "Music" } as const;

/** What the transport buttons send. Errors surface as the next reading not changing. */
function send(now: NowPlaying, transport: Transport) {
  try {
    void commands.mediaControl(now.player, transport);
  } catch {
    /* browser preview */
  }
}

/**
 * Media › Now Playing: what Spotify or Music is playing, with the controls.
 * The closed notch shows the same track as an island; this is it one step
 * deeper (scrub, previous, next).
 */
export function MediaPlaying() {
  const { reading, readAt, patch } = useNowPlaying();
  const { now, denied } = reading;
  const art = useArtwork(now);

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
        macOS asks once before one app may drive another. Allow Aloud under
        Privacy &amp; Security › Automation, and the track shows up here.
      </Empty>
    );
  }
  if (!now) {
    return (
      <Empty title="Nothing playing">
        Play something in Spotify or Music. It shows up here, and beside the
        camera while the notch is closed.
      </Empty>
    );
  }

  const playing = now.state === "playing";
  return (
    <div className="flex items-center gap-4 font-notch">
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
          <MusicIcon className="size-6" strokeWidth={1.6} />
        </span>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <Lead
          sub={
            <span className="block truncate">
              {[now.artist, now.album].filter(Boolean).join(" · ")}
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
            aria-label="Previous track"
            onClick={() => send(now, "previous")}
          >
            <SkipBackIcon />
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
            aria-label="Next track"
            onClick={() => send(now, "next")}
          >
            <SkipForwardIcon />
          </Button>
          <Badge variant="notch" className="ml-auto">
            {PLAYER_NAME[now.player]}
          </Badge>
        </div>
      </div>
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
