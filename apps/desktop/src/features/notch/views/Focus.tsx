import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { TextArea } from "@/components/notch/field";
import { Empty, Label, Lead, List, Row } from "@/components/notch/list";
import { Ring } from "@/components/notch/meters";
import { Segmented } from "@/components/notch/segmented";
import { commands, type FocusDay, type Note, type TimerMode, type TimerState } from "@/lib/bindings";
import { createAutosaver } from "@/lib/autosave";
import { formatDate } from "@/lib/format";
import { clock, timerTitle, timerValueMs } from "@/lib/focus";
import { cn } from "@/lib/utils";
import { useFocus, useFocusStats } from "../useFocus";

const PRESETS = [
  { id: "25-5", label: "25 / 5", focus: 25, rest: 5 },
  { id: "50-10", label: "50 / 10", focus: 50, rest: 10 },
];

const MODES = [
  { id: "pomodoro", label: "Pomodoro" },
  { id: "stopwatch", label: "Stopwatch" },
];

/** What the one inverted button says for a timer in this state. */
function primaryLabel(s: TimerState | undefined): string {
  switch (s?.status) {
    case "running":
      return "Pause";
    case "paused":
      return "Resume";
    case "ready":
      return s.phase === "focus" ? "Start focus" : "Start break";
    default:
      return "Start";
  }
}

/**
 * Focus › Timer: a Pomodoro with its rounds, or a stopwatch. Rust keeps the
 * time against the wall clock; this view only reads it, so closing the panel,
 * sleeping the Mac or a slow render never moves the time.
 */
export function FocusTimer() {
  const { snap, now } = useFocus();
  const timer = snap?.timer;
  const idle = !timer || timer.status === "idle";
  const [mode, setMode] = useState<TimerMode>("pomodoro");
  const [preset, setPreset] = useState(PRESETS[0].id);
  const chosen = PRESETS.find((p) => p.id === preset) ?? PRESETS[0];
  const shown: TimerMode = idle ? mode : timer.mode;
  const countdown = shown === "pomodoro";

  const value = idle ? (countdown ? chosen.focus * 60_000 : 0) : timerValueMs(timer, now);
  const total = idle ? chosen.focus * 60_000 : timer.totalMs;
  const stats = useFocusStats(`${timer?.status}${timer?.phase}${timer?.round}${timer?.mode}`);

  const primary = () => {
    if (idle) return commands.focusStart(mode, { focus: chosen.focus, rest: chosen.rest });
    return timer.status === "running" ? commands.focusPause() : commands.focusResume();
  };

  return (
    <div className="flex items-center gap-7">
      <div className="grid size-[132px] shrink-0 place-items-center">
        {countdown ? (
          <Ring
            size={132}
            stroke={7}
            // The ring drains as the time goes: what is left, not what is spent.
            value={total > 0 ? value / total : 0}
            label={timer ? timerTitle(timer) : "Focus timer"}
          >
            <Lead sub={timer ? timerTitle(timer) : "Focus"} className="text-center">
              {clock(value, true)}
            </Lead>
          </Ring>
        ) : (
          <Lead sub="Stopwatch" className="text-center">
            {clock(value, false)}
          </Lead>
        )}
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-3">
        {idle ? (
          <div className="flex flex-wrap items-center gap-2">
            <Segmented
              label="Timer type"
              items={MODES}
              value={mode}
              onChange={(id) => setMode(id as TimerMode)}
            />
            {mode === "pomodoro" && (
              <Segmented
                label="Focus and break minutes"
                items={PRESETS}
                value={preset}
                onChange={setPreset}
              />
            )}
          </div>
        ) : (
          <p className="m-0 text-notch-body text-notch-ink-2">
            {timer.status === "ready"
              ? `${timerTitle(timer)} is ready when you are.`
              : timer.status === "paused"
                ? "Paused."
                : countdown
                  ? `${timer.preset.focus} minutes of focus, then ${timer.preset.rest} to rest.`
                  : "Counting up."}
          </p>
        )}

        <div className="flex items-center gap-1.5">
          <Button variant="notch" size="notch" onClick={() => void primary()}>
            {primaryLabel(timer)}
          </Button>
          {!idle && countdown && (
            <Button variant="notch-quiet" size="notch" onClick={() => void commands.focusSkip()}>
              Skip
            </Button>
          )}
          {!idle && (
            <Button variant="notch-quiet" size="notch" onClick={() => void commands.focusStop()}>
              Stop
            </Button>
          )}
        </div>

        {stats && <Week days={stats.week} streak={stats.streak} />}
      </div>
    </div>
  );
}

const DAY_INITIAL = ["S", "M", "T", "W", "T", "F", "S"];

/** Seven days of finished focus, today last, and the streak in words. */
function Week({ days, streak }: { days: FocusDay[]; streak: number }) {
  // At least an hour of headroom, so one short session is not a full bar.
  const top = Math.max(60, ...days.map((d) => d.minutes));
  const today = days[days.length - 1];
  return (
    <div className="flex items-end gap-4">
      <div className="flex items-end gap-1.5" role="group" aria-label="Focus this week">
        {days.map((d, i) => {
          const dow = new Date(`${d.day}T12:00:00`).getDay();
          const last = i === days.length - 1;
          return (
            <div key={d.day} className="flex flex-col items-center gap-1">
              <div
                role="img"
                aria-label={`${d.day}: ${d.sessions} sessions, ${d.minutes} minutes`}
                className="flex h-7 w-[14px] items-end rounded-notch-key bg-notch-track"
              >
                <div
                  className={cn("w-full rounded-notch-key", last ? "bg-notch-ink" : "bg-notch-ink-3")}
                  style={{ height: `${Math.max(d.minutes > 0 ? 12 : 0, (d.minutes / top) * 100)}%` }}
                />
              </div>
              <span className="text-notch-label text-notch-ink-3">{DAY_INITIAL[dow]}</span>
            </div>
          );
        })}
      </div>
      <p className="m-0 pb-4 text-notch-meta text-notch-ink-2">
        {streak > 0 ? `${streak}-day streak` : "No streak yet"}
        {today.sessions > 0 && ` · ${today.sessions} today`}
      </p>
    </div>
  );
}

const AWAKE_CHOICES = [
  { id: "off", label: "Off" },
  { id: "30", label: "30 min" },
  { id: "120", label: "2 hrs" },
  { id: "480", label: "8 hrs" },
  { id: "always", label: "Always" },
];

/**
 * Focus › Keep Awake: `caffeinate` while it is on, and the closed notch says
 * so. The display stays on too, which is the point for a call or a long read.
 */
export function FocusAwake() {
  const { snap } = useFocus();
  const [error, setError] = useState<string | null>(null);
  const awake = snap?.awake;
  const choice = !awake?.active ? "off" : awake.minutes == null ? "always" : String(awake.minutes);

  const choose = async (id: string) => {
    setError(null);
    if (id === "off") return void commands.awakeStop();
    const r = await commands.awakeStart(id === "always" ? null : Number(id));
    if (r.status === "error") setError(r.error);
  };

  const until =
    awake?.active && awake.until != null
      ? new Date(awake.until).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
      : null;

  return (
    <div className="flex flex-col gap-4">
      <Lead
        sub={
          awake?.active
            ? `${until ? `Until ${until}` : "Until you turn it off"} · the display stays on`
            : "Your Mac sleeps on its usual schedule."
        }
      >
        {awake?.active ? "Awake" : "Off"}
      </Lead>
      <Segmented label="Keep awake for" items={AWAKE_CHOICES} value={choice} onChange={(id) => void choose(id)} />
      {error && <p className="m-0 text-notch-meta text-notch-danger">{error}</p>}
    </div>
  );
}

/** First non-empty line, without markdown's `#`: what a note is called in the list. */
function noteTitle(text: string): string {
  const line = text.split("\n").find((l) => l.trim() !== "") ?? "";
  return line.replace(/^#+\s*/, "").trim() || "Empty note";
}

/**
 * Focus › Notes: type, and it is saved — a markdown file in the notes folder
 * (the same one voice notes and meetings use), written after every pause and
 * when you leave. No save button, no title to invent.
 */
export function FocusNotes() {
  const [text, setText] = useState("");
  const [notes, setNotes] = useState<Note[]>([]);
  const path = useRef<string | null>(null);
  const latest = useRef("");

  const reload = () =>
    commands
      .quickNotes()
      .then((r) => r.status === "ok" && setNotes(r.data.filter((n) => n.content.trim() !== "")))
      .catch(() => {});

  const [saver] = useState(() =>
    createAutosaver(async (body) => {
      // A blank note that was never saved is not worth a file.
      if (path.current === null && body.trim() === "") return;
      const r = await commands.saveQuickNote(path.current, body);
      if (r.status === "ok") {
        path.current = r.data;
        void reload();
      }
    }),
  );

  useEffect(() => {
    void reload();
    // Leaving the tab or closing the panel inside the debounce must not lose
    // the last words typed.
    return () => void saver.flush();
  }, [saver]);

  const edit = (body: string) => {
    setText(body);
    latest.current = body;
    saver.schedule(body);
  };

  const open = async (note: Note) => {
    await saver.flush();
    path.current = note.path;
    latest.current = note.content;
    setText(note.content);
  };

  const fresh = async () => {
    await saver.flush();
    path.current = null;
    latest.current = "";
    setText("");
  };

  return (
    <div className="flex flex-col gap-3">
      <TextArea
        aria-label="Quick note"
        value={text}
        onChange={(e) => edit(e.target.value)}
        placeholder="Type a thought. It saves itself."
        rows={4}
      />
      {notes.length === 0 ? (
        <Empty title="No quick notes yet">
          Notes you type here are kept as markdown in your Aloud notes folder, beside your meeting
          notes.
        </Empty>
      ) : (
        <div>
          <div className="flex items-center justify-between">
            <Label>Recent</Label>
            <Button variant="notch-quiet" size="notch" className="-mt-1.5" onClick={() => void fresh()}>
              New note
            </Button>
          </div>
          <List label="Quick notes">
            {notes.slice(0, 5).map((note) => (
              <Row
                key={note.path}
                title={noteTitle(note.content)}
                subtitle={formatDate(note.date)}
                selected={note.path === path.current}
                onClick={() => void open(note)}
              />
            ))}
          </List>
        </div>
      )}
    </div>
  );
}
