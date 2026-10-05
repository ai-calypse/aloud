import { useState } from "react";
import { CoffeeIcon, FolderIcon, TimerIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import {
  Numeral,
  Ring,
  WidgetBoard,
  WidgetFrame,
  formatNumeral,
} from "@/components/notch/widgets";
import {
  commands,
  type FocusSnapshot,
  type FocusStats,
  type Note,
  type TimerMode,
  type TimerState,
} from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { useFocus, useFocusStats } from "../../useFocus";
import { Choice, Idle, Pill } from "./parts";
import {
  AWAKE_CHOICES,
  MODES,
  PRESETS,
  awakeChoice,
  noteSnippet,
  noteTitle,
  noteWhen,
  primaryLabel,
  timerCaption,
  timerFace,
  weekWidget,
} from "./focus-widgets";
import { useQuickNotes } from "./useQuickNotes";

/* Focus › Timer ---------------------------------------------------------- */

/**
 * Focus › Timer in the widgets look: the big ring draining in Move red, the
 * calm view's controls on the tile beside it, and the streak and week as small
 * tiles. Rust keeps the time against the wall clock; this only reads it.
 */
export function WidgetFocusTimer() {
  const { snap, now } = useFocus();
  const timer = snap?.timer;
  const idle = !timer || timer.status === "idle";
  const [mode, setMode] = useState<TimerMode>("pomodoro");
  const [preset, setPreset] = useState(PRESETS[0].id);
  const chosen = PRESETS.find((p) => p.id === preset) ?? PRESETS[0];
  const stats = useFocusStats(`${timer?.status}${timer?.phase}${timer?.round}${timer?.mode}`);

  const primary = () => {
    if (idle) return commands.focusStart(mode, { focus: chosen.focus, rest: chosen.rest });
    return timer.status === "running" ? commands.focusPause() : commands.focusResume();
  };

  return (
    <TimerBoard
      timer={timer}
      now={now}
      stats={stats}
      mode={mode}
      onMode={setMode}
      preset={preset}
      onPreset={setPreset}
      onPrimary={() => void primary()}
      onSkip={() => void commands.focusSkip()}
      onStop={() => void commands.focusStop()}
    />
  );
}

export function TimerBoard({
  timer,
  now,
  stats,
  mode,
  onMode,
  preset,
  onPreset,
  onPrimary,
  onSkip,
  onStop,
}: {
  timer: TimerState | undefined;
  now: number;
  stats: FocusStats | null;
  mode: TimerMode;
  onMode: (m: TimerMode) => void;
  preset: string;
  onPreset: (id: string) => void;
  onPrimary: () => void;
  onSkip: () => void;
  onStop: () => void;
}) {
  const chosen = PRESETS.find((p) => p.id === preset) ?? PRESETS[0];
  const face = timerFace(timer, now, mode, chosen.focus);
  const running = timer?.status === "running";
  const week = stats && weekWidget(stats.week, stats.streak);

  return (
    <WidgetBoard label="Timer">
      <WidgetFrame size="medium" label="Timer" accent={face.accent} ground="deep">
        <div className="flex h-full items-center gap-4 p-3.5">
          <div className="grid size-31.5 shrink-0 place-items-center">
            {face.countdown ? (
              <Ring
                value={face.value}
                label={face.idle ? "Focus timer" : face.title}
                valueText={`${face.text}, ${face.title}`}
                size={126}
                stroke={14}
                // A fixed ride the clock owns, so the CSS drain, not a spring
                // that would chase a value nothing redirects.
                motion={running ? "drain" : "none"}
              >
                <Numeral text={face.text} size="s" className="text-widget-acc-ink" />
                <div className="mt-0.5 text-notch-w-over leading-none font-semibold text-widget-ink-2">
                  {face.title}
                </div>
              </Ring>
            ) : (
              <div className="text-center">
                <Numeral text={face.text} size="s" />
                <div className="mt-0.5 text-notch-w-over leading-none font-semibold text-widget-ink-2">
                  {face.title}
                </div>
              </div>
            )}
          </div>

          <div className="flex min-w-0 flex-1 flex-col justify-center gap-2.5">
            {!timer || timer.status === "idle" ? (
              <div className="flex flex-col gap-1.5">
                <Choice label="Timer type" items={MODES} value={mode} onChange={(id) => onMode(id as TimerMode)} />
                {mode === "pomodoro" && (
                  <Choice label="Focus and break minutes" items={PRESETS} value={preset} onChange={onPreset} />
                )}
              </div>
            ) : (
              <p className="m-0 text-notch-w-title leading-snug font-medium text-widget-ink-2">
                {timerCaption(timer)}
              </p>
            )}
            <div className="flex flex-wrap items-center gap-1.5">
              <Pill primary onClick={onPrimary}>
                {primaryLabel(timer)}
              </Pill>
              {timer && timer.status !== "idle" && timer.mode === "pomodoro" && (
                <Pill onClick={onSkip}>Skip</Pill>
              )}
              {timer && timer.status !== "idle" && <Pill onClick={onStop}>Stop</Pill>}
            </div>
          </div>
        </div>
      </WidgetFrame>

      {week && stats && (
        <>
          <WidgetFrame size="small" label="Streak" accent="exercise">
            <div className="flex h-full flex-col justify-between p-3.5">
              <TimerIcon aria-hidden className="size-6 text-widget-acc-ink" strokeWidth={1.8} />
              <div>
                {stats.streak > 0 ? (
                  <Numeral text={formatNumeral(stats.streak)} size="l" className="text-widget-acc-ink" />
                ) : null}
                <div className="text-notch-w-title font-semibold text-widget-ink-2">{week.streakWord}</div>
                {week.todaySessions > 0 && (
                  <div className="text-notch-w-meta font-medium text-widget-ink-2">
                    {week.todaySessions} today
                  </div>
                )}
              </div>
            </div>
          </WidgetFrame>
          <WidgetFrame size="small" label="This week" accent="stand">
            <div className="flex h-full flex-col justify-between p-3.5">
              <div role="group" aria-label="Focus this week" className="flex h-20 items-end justify-between gap-1">
                {week.bars.map((bar) => (
                  <div key={bar.key} className="flex h-full flex-1 flex-col items-center gap-1">
                    <div
                      role="img"
                      aria-label={bar.label}
                      className="flex w-full flex-1 items-end overflow-hidden rounded-full bg-widget-track"
                    >
                      <div
                        className={cn("w-full rounded-full", bar.today ? "bg-widget-acc" : "bg-widget-acc/60")}
                        style={{ height: `${bar.height * 100}%` }}
                      />
                    </div>
                    <span className="text-notch-w-over leading-none font-semibold text-widget-ink-2">
                      {bar.initial}
                    </span>
                  </div>
                ))}
              </div>
              <div className="text-notch-w-title leading-tight font-semibold text-widget-acc-ink">
                {week.totalWord}
              </div>
            </div>
          </WidgetFrame>
        </>
      )}
    </WidgetBoard>
  );
}

/* Focus › Keep Awake ----------------------------------------------------- */

/** Focus › Keep Awake in the widgets look: Coffee, the state in a word, and the durations. */
export function WidgetFocusAwake() {
  const { snap } = useFocus();
  const [error, setError] = useState<string | null>(null);
  const choose = async (id: string) => {
    setError(null);
    if (id === "off") return void commands.awakeStop();
    const r = await commands.awakeStart(id === "always" ? null : Number(id));
    if (r.status === "error") setError(r.error);
  };
  return <AwakeBoard snap={snap} error={error} onChoose={(id) => void choose(id)} />;
}

export function AwakeBoard({
  snap,
  error,
  onChoose,
}: {
  snap: FocusSnapshot | null;
  error: string | null;
  onChoose: (id: string) => void;
}) {
  const awake = snap?.awake;
  const on = !!awake?.active;
  const until =
    on && awake.until != null
      ? new Date(awake.until).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
      : null;
  return (
    <WidgetBoard label="Keep awake">
      {/* Filled orange while on: the one tile in the look that says "something is held". */}
      <WidgetFrame size="medium" label="Keep awake" accent="orange" ground={on ? "accent" : "tile"}>
        <div className="flex h-full gap-4 p-3.5">
          <div className="flex min-w-0 flex-1 flex-col justify-between">
            <CoffeeIcon
              aria-hidden
              className={cn("size-7", on ? "text-widget-ink" : "text-widget-ink-2")}
              strokeWidth={1.6}
            />
            <div>
              <div className="text-notch-w-numeral-s leading-none font-light">{on ? "Awake" : "Off"}</div>
              <div className="mt-1 text-notch-w-title leading-snug font-medium text-widget-ink/85">
                {on
                  ? `${until ? `Until ${until}` : "Until you turn it off"} · the display stays on`
                  : "Your Mac sleeps on its usual schedule."}
              </div>
              {error && <p className="m-0 mt-1 text-notch-w-meta text-widget-ink">{error}</p>}
            </div>
          </div>
          <div className="flex w-32 shrink-0 flex-col justify-center">
            <Choice
              label="Keep awake for"
              items={AWAKE_CHOICES}
              value={awakeChoice(awake)}
              onChange={onChoose}
              className="grid grid-cols-2 gap-1.5"
            />
          </div>
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}

/* Focus › Notes ---------------------------------------------------------- */

/**
 * Focus › Notes in the widgets look: the Notes widget's yellow header over the
 * recent notes, and a second tile to write in. Autosave, opening an older
 * note and starting a new one are the calm view's, from one shared hook.
 */
export function WidgetFocusNotes() {
  const q = useQuickNotes();
  return (
    <NotesBoard
      text={q.text}
      notes={q.notes}
      openPath={q.openPath}
      now={Date.now()}
      onEdit={q.edit}
      onOpen={(n) => void q.open(n)}
      onNew={() => void q.fresh()}
    />
  );
}

/** Dark words on Notes yellow: Apple's white on #FFD409 is 1.4:1. */
const NOTES_HEAD =
  "flex h-11 shrink-0 items-center gap-2 bg-widget-acc px-4 text-notch-w-headline font-semibold text-widget-on";

export function NotesBoard({
  text,
  notes,
  openPath,
  now,
  onEdit,
  onOpen,
  onNew,
}: {
  text: string;
  notes: Note[];
  openPath: string | null;
  now: number;
  onEdit: (body: string) => void;
  onOpen: (note: Note) => void;
  onNew: () => void;
}) {
  return (
    <WidgetBoard label="Notes">
      <WidgetFrame size="large" label="Recent notes" accent="sun">
        <div className="flex h-full flex-col">
          <div className={NOTES_HEAD}>
            <FolderIcon aria-hidden className="size-5" fill="currentColor" strokeWidth={1.6} />
            Notes
          </div>
          {notes.length === 0 ? (
            <div className="min-h-0 flex-1">
              <Idle icon={<FolderIcon strokeWidth={1.6} />}>
                No quick notes yet. Notes you type here are kept as markdown in your Aloud notes
                folder, beside your meeting notes.
              </Idle>
            </div>
          ) : (
            <ul aria-label="Quick notes" className="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0">
              {notes.slice(0, 5).map((note) => {
                const selected = note.path === openPath;
                return (
                  <li key={note.path} className="border-b border-widget-sep last:border-b-0">
                    <button
                      type="button"
                      aria-current={selected || undefined}
                      onClick={() => onOpen(note)}
                      className={cn(
                        NOTCH_FOCUS,
                        "block w-full px-4 py-2.5 text-left focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
                        selected && "bg-widget-tile-2",
                      )}
                    >
                      <span className="block truncate text-notch-w-body leading-tight font-medium">
                        {noteTitle(note.content)}
                      </span>
                      <span className="mt-0.5 block truncate text-notch-w-title leading-tight text-widget-ink-2">
                        {noteWhen(note.date, now)}
                        {noteSnippet(note.content) && `  ${noteSnippet(note.content)}`}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </WidgetFrame>
      <WidgetFrame size="large" label="Quick note" accent="sun">
        <div className="flex h-full flex-col">
          <div className={cn(NOTES_HEAD, "justify-between")}>
            <span className="flex items-center gap-2">
              <FolderIcon aria-hidden className="size-5" fill="currentColor" strokeWidth={1.6} />
              Quick note
            </span>
            <button
              type="button"
              onClick={onNew}
              className={cn(
                NOTCH_FOCUS,
                "h-6.5 rounded-full bg-widget-on/15 px-3 text-notch-w-meta font-semibold transition-transform duration-[var(--dur-fast)] ease-[var(--ease-out)] active:scale-97",
              )}
            >
              New note
            </button>
          </div>
          <textarea
            aria-label="Quick note"
            value={text}
            onChange={(e) => onEdit(e.target.value)}
            placeholder="Type a thought. It saves itself."
            spellCheck={false}
            className={cn(
              NOTCH_FOCUS,
              "min-h-0 flex-1 resize-none bg-transparent p-4 text-notch-w-body leading-snug text-widget-ink focus-visible:-outline-offset-2 placeholder:text-widget-ink-3",
            )}
          />
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}
