import { useEffect, useRef, useState } from "react";
import { commands, type Lyrics, type NowPlaying } from "@/lib/bindings";
import { createSpring, type Spring } from "@/lib/spring";
import { Button } from "@/components/ui/button";
import { Empty, Label } from "@/components/notch/list";
import { cn } from "@/lib/utils";
import { lineAt, positionAt, useNowPlaying } from "../media";

type Load =
  | { key: string; status: "loading" }
  | { key: string; status: "error"; message: string }
  | { key: string; status: "ready"; lyrics: Lyrics | null };

/** Names the track, not the moment: what lyrics are keyed on. */
function keyOf(now: NowPlaying) {
  return `${now.player}|${now.title}|${now.artist}|${Math.floor(now.durationMs / 1000)}`;
}

/**
 * Media › Lyrics: the line being sung, kept in view as the track plays.
 * LRCLIB is asked from Rust (the overlay's CSP allows no network), once per
 * track; the answer is cached on disk.
 */
export function MediaLyrics() {
  const { reading, readAt } = useNowPlaying();
  const { now } = reading;
  const [load, setLoad] = useState<Load | null>(null);
  const [attempt, setAttempt] = useState(0);
  const latest = useRef(now);
  latest.current = now;
  const key = now ? keyOf(now) : null;

  useEffect(() => {
    const track = latest.current;
    if (!key || !track) return;
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

  if (!now) {
    return (
      <Empty title="No lyrics without a song">
        Play something in Spotify or Music and its lyrics scroll here in time.
      </Empty>
    );
  }
  // A load for the last track must never be read as this one's.
  const current = load && load.key === key ? load : null;
  const heading = (
    <Label>{[now.title, now.artist].filter(Boolean).join(" · ")}</Label>
  );

  if (!current || current.status === "loading") {
    return (
      <>
        {heading}
        <p className="m-0 font-notch text-notch-body text-notch-ink-3">Finding the lyrics…</p>
      </>
    );
  }
  if (current.status === "error") {
    return (
      <Empty
        title="Couldn't reach the lyrics service"
        action={
          <Button variant="notch-quiet" size="notch" className="-ml-2.5" onClick={() => setAttempt((n) => n + 1)}>
            Try again
          </Button>
        }
      >
        {current.message}
      </Empty>
    );
  }
  const lyrics = current.lyrics;
  if (!lyrics || (!lyrics.lines.length && !lyrics.plain && !lyrics.instrumental)) {
    return (
      <>
        {heading}
        <Empty title="No lyrics for this track">
          LRCLIB has none for it yet. Aloud asks again tomorrow.
        </Empty>
      </>
    );
  }
  if (lyrics.instrumental) {
    return (
      <>
        {heading}
        <Empty title="Instrumental">There are no words to follow.</Empty>
      </>
    );
  }
  return (
    <>
      {heading}
      {lyrics.lines.length > 0 ? (
        // Keyed on the track so a new song starts from its own first line.
        <Synced key={key} lines={lyrics.lines} now={now} readAt={readAt} />
      ) : (
        <div className="max-h-[260px] overflow-y-auto font-notch text-notch-body whitespace-pre-line text-notch-ink-2">
          {lyrics.plain}
          <p className="mt-3 mb-0 text-notch-meta text-notch-ink-3">Not timed, so it does not scroll.</p>
        </div>
      )}
    </>
  );
}

/** Quarter-second ticks: a line is held for seconds, so this is early enough. */
const TICK_MS = 250;

/**
 * The timed lines. The current one is the view's lead (ink, the rest Ink 3),
 * every line the same size so a change of line moves nothing but the scroll.
 * The scroll is a live spring: lines can change faster than it settles, and a
 * seek retargets it mid-flight, so a baked CSS curve would restart from rest.
 */
function Synced({
  lines,
  now,
  readAt,
}: {
  lines: Lyrics["lines"];
  now: NowPlaying;
  readAt: number;
}) {
  const [, tick] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const rows = useRef<(HTMLParagraphElement | null)[]>([]);
  const spring = useRef<Spring | null>(null);
  const placed = useRef(false);
  const playing = now.state === "playing";
  // -1 (before the first line) rests on the first line, unhighlighted.
  const index = lineAt(lines, positionAt(now, readAt, performance.now()));

  useEffect(() => {
    if (!playing) return;
    const timer = setInterval(() => tick((n) => n + 1), TICK_MS);
    return () => clearInterval(timer);
  }, [playing]);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const s = createSpring((top) => {
      el.scrollTop = top;
    }, { response: 0.5 });
    spring.current = s;
    return () => s.stop();
  }, []);

  useEffect(() => {
    const el = box.current;
    const row = rows.current[Math.max(index, 0)];
    if (!el || !row || !spring.current) return;
    const top = row.offsetTop - (el.clientHeight - row.offsetHeight) / 2;
    // The first placement is where the song already is: no travel from the top.
    if (placed.current) spring.current.to(top);
    else spring.current.jump(top);
    placed.current = true;
  }, [index]);

  return (
    <div
      ref={box}
      className="relative h-[260px] overflow-hidden font-notch"
      role="region"
      aria-label="Lyrics"
    >
      <div aria-hidden className="h-[110px]" />
      {lines.map((line, i) => (
        <p
          key={`${line.ms}-${i}`}
          ref={(el) => {
            rows.current[i] = el;
          }}
          aria-current={i === index || undefined}
          className={cn(
            "m-0 py-1.5 text-notch-lead leading-[1.15] font-semibold tracking-[-0.035em] text-pretty",
            "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
            i === index ? "text-notch-ink" : "text-notch-ink-3",
          )}
        >
          {line.text || "♪"}
        </p>
      ))}
      <div aria-hidden className="h-[110px]" />
    </div>
  );
}
