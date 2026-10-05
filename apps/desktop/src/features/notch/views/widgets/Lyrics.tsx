import { useEffect, useRef, useState } from "react";
import { MicVocalIcon } from "lucide-react";
import { commands, type Lyrics, type NowPlaying } from "@/lib/bindings";
import { createSpring, type Spring } from "@/lib/spring";
import { WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { cn } from "@/lib/utils";
import { useNotch } from "../../context";
import { lineAt, needsSong, positionAt, useArtwork, useNowPlaying } from "../../media";
import { lineTone, restLine, sourceRole, type LineTone } from "./media-format";
import { Notice, Pill } from "./parts";
import { usePlayerControls } from "./playback";
import { MusicTile } from "../WidgetHome";

export type Load =
  | { key: string; status: "loading" }
  | { key: string; status: "error"; message: string }
  | { key: string; status: "ready"; lyrics: Lyrics | null };

/** Names the track, not the moment: what lyrics are keyed on. */
export function keyOf(now: NowPlaying) {
  return `${now.player}|${now.title}|${now.artist}|${Math.floor(now.durationMs / 1000)}`;
}

/**
 * Media › Lyrics in the widgets look: a large tile of the synced lines, the
 * one being sung bright, with the Music widget beside it to pause from. The
 * same lookup as the calm view: LRCLIB is asked from Rust (the overlay's CSP
 * allows no network), once per track, and cached on disk.
 */
export function WidgetMediaLyrics() {
  const { navigate } = useNotch();
  const { reading, readAt, patch } = useNowPlaying();
  const { now } = reading;
  const art = useArtwork(now);
  const controls = usePlayerControls(now, readAt, patch);
  const [load, setLoad] = useState<Load | null>(null);
  const [attempt, setAttempt] = useState(0);
  const latest = useRef(now);
  latest.current = now;
  const key = now ? keyOf(now) : null;

  useEffect(() => {
    const track = latest.current;
    // A web video the page does not name a song for has nothing to look up.
    if (!key || !track || (needsSong(track) && !track.song)) return;
    let live = true;
    setLoad({ key, status: "loading" });
    try {
      commands
        .mediaLyrics(track)
        .then((res) => {
          if (!live) return;
          setLoad(
            res.status === "ok"
              ? { key, status: "ready", lyrics: res.data }
              : { key, status: "error", message: res.error },
          );
        })
        .catch(() => {});
    } catch {
      /* browser preview: no Tauri */
    }
    return () => {
      live = false;
    };
  }, [key, attempt]);

  return (
    <LyricsBoard
      now={now}
      readAt={readAt}
      // A load for the last track must never be read as this one's.
      load={load && load.key === key ? load : null}
      art={art}
      onToggle={controls.toggle}
      onRetry={() => setAttempt((n) => n + 1)}
      onOpenPlaying={() => navigate("media", "playing")}
    />
  );
}

export function LyricsBoard({
  now,
  readAt,
  load,
  art,
  onToggle,
  onRetry,
  onOpenPlaying,
}: {
  now: NowPlaying | null;
  readAt: number;
  load: Load | null;
  art: string | null;
  onToggle: () => void;
  onRetry: () => void;
  onOpenPlaying?: () => void;
}) {
  return (
    <WidgetBoard label="Lyrics">
      <LyricsTile now={now} readAt={readAt} load={load} onRetry={onRetry} />
      {now && <MusicTile now={now} art={art} onToggle={onToggle} onOpen={onOpenPlaying} />}
    </WidgetBoard>
  );
}

const ICON = <MicVocalIcon strokeWidth={1.6} />;

function LyricsTile({
  now,
  readAt,
  load,
  onRetry,
}: {
  now: NowPlaying | null;
  readAt: number;
  load: Load | null;
  onRetry: () => void;
}) {
  if (!now) {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice icon={ICON} title="No lyrics without a song">
          Play a song in any music app and its lyrics scroll here in time.
        </Notice>
      </WidgetFrame>
    );
  }
  if (needsSong(now) && !now.song) {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice icon={ICON} title="No lyrics for this one">
          Aloud looks lyrics up only when it knows the title and artist are a song: a music app, or a web page that
          names them cleanly (YouTube Music, an artist&apos;s own upload titled &ldquo;Artist - Song&rdquo;, or an
          auto-generated Topic channel).
        </Notice>
      </WidgetFrame>
    );
  }
  const eyebrow = [now.title, now.artist].filter(Boolean).join(" · ");
  if (!load || load.status === "loading") {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice icon={ICON} eyebrow={eyebrow} title="Finding the lyrics…" />
      </WidgetFrame>
    );
  }
  if (load.status === "error") {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice
          icon={ICON}
          title="Couldn't reach the lyrics service"
          actions={
            <Pill solid onClick={onRetry}>
              Try again
            </Pill>
          }
        >
          <p className="m-0">{load.message}</p>
        </Notice>
      </WidgetFrame>
    );
  }
  const lyrics = load.lyrics;
  if (!lyrics || (!lyrics.lines.length && !lyrics.plain && !lyrics.instrumental)) {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice icon={ICON} eyebrow={eyebrow} title="No lyrics for this track">
          LRCLIB has none for it yet. Aloud asks again tomorrow.
        </Notice>
      </WidgetFrame>
    );
  }
  if (lyrics.instrumental) {
    return (
      <WidgetFrame size="large" label="Lyrics">
        <Notice icon={ICON} eyebrow={eyebrow} title="Instrumental">
          There are no words to follow.
        </Notice>
      </WidgetFrame>
    );
  }
  const role = sourceRole(now);
  return (
    <WidgetFrame size="large" label="Lyrics" accent={role} ground="accent">
      <div className="flex h-full flex-col gap-2 p-3.5">
        <span className="truncate text-notch-w-over font-semibold tracking-wide text-widget-ink/80 uppercase">
          {eyebrow}
        </span>
        {lyrics.lines.length > 0 ? (
          // Keyed on the track so a new song starts from its own first line.
          <Synced key={keyOf(now)} lines={lyrics.lines} now={now} readAt={readAt} />
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto text-notch-w-body leading-snug font-medium whitespace-pre-line">
            {lyrics.plain}
            <p className="mt-3 mb-0 text-notch-w-meta text-widget-ink/85">Not timed, so it does not scroll.</p>
          </div>
        )}
      </div>
    </WidgetFrame>
  );
}

/** Quarter-second ticks: a line is held for seconds, so this is early enough. */
const TICK_MS = 250;

const TONE: Record<LineTone, string> = {
  current: "text-widget-ink",
  near: "text-widget-ink/75",
  far: "text-widget-ink/60",
};

/**
 * The timed lines. The one being sung is full ink and its neighbours step down,
 * every line the same size so a change of line moves nothing but the scroll.
 * The scroll is a live spring: lines can change faster than it settles, and a
 * seek retargets it mid-flight, so a baked CSS curve would restart from rest.
 */
function Synced({ lines, now, readAt }: { lines: Lyrics["lines"]; now: NowPlaying; readAt: number }) {
  const [, tick] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const rows = useRef<(HTMLParagraphElement | null)[]>([]);
  const spring = useRef<Spring | null>(null);
  const placed = useRef(false);
  const playing = now.state === "playing";
  // -1 (before the first line) rests on the first line, unlit.
  const index = lineAt(lines, positionAt(now, readAt, performance.now()));

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(() => tick((n) => n + 1), TICK_MS);
    return () => clearInterval(timer);
  }, [playing]);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const s = createSpring(
      (top) => {
        el.scrollTop = top;
      },
      { response: 0.5 },
    );
    spring.current = s;
    return () => s.stop();
  }, []);

  useEffect(() => {
    const el = box.current;
    const row = rows.current[restLine(index)];
    if (!el || !row || !spring.current) return;
    const top = row.offsetTop - (el.clientHeight - row.offsetHeight) / 2;
    // The first placement is where the song already is: no travel from the top.
    if (placed.current) spring.current.to(top);
    else spring.current.jump(top);
    placed.current = true;
  }, [index]);

  return (
    <div ref={box} className="relative min-h-0 flex-1 overflow-hidden" role="region" aria-label="Lyrics">
      <div aria-hidden className="h-28" />
      {lines.map((line, i) => (
        <p
          key={`${line.ms}-${i}`}
          ref={(el) => {
            rows.current[i] = el;
          }}
          aria-current={i === index || undefined}
          className={cn(
            "m-0 py-1.5 text-notch-w-headline leading-tight font-bold text-pretty",
            "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
            TONE[lineTone(i, index)],
          )}
        >
          {line.text || "♪"}
        </p>
      ))}
      <div aria-hidden className="h-28" />
    </div>
  );
}
