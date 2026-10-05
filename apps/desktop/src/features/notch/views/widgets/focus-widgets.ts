import type { AwakeState, FocusDay, TimerMode, TimerState } from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets";
import { clock, timerTitle, timerValueMs } from "@/lib/focus";
import { formatMinutes } from "../mac-format";

/**
 * What the Focus widgets say. These mirror the calm Focus view's rules
 * (presets, labels, headroom) on purpose: the look changes the drawing, never
 * what a button does or what a number means.
 */

export const PRESETS = [
  { id: "25-5", label: "25 / 5", focus: 25, rest: 5 },
  { id: "50-10", label: "50 / 10", focus: 50, rest: 10 },
];

export const MODES = [
  { id: "pomodoro", label: "Pomodoro" },
  { id: "stopwatch", label: "Stopwatch" },
];

export const AWAKE_CHOICES = [
  { id: "off", label: "Off" },
  { id: "30", label: "30 min" },
  { id: "120", label: "2 hrs" },
  { id: "480", label: "8 hrs" },
  { id: "always", label: "Always" },
];

/** What the one primary button says for a timer in this state. */
export function primaryLabel(s: TimerState | undefined): string {
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
 * The timer ring's face. Idle shows the chosen preset full; a countdown's ring is what is *left* (it drains), and a stopwatch has
 * no end to drain towards, so it has numerals and no ring.
 */
export function timerFace(
  timer: TimerState | undefined,
  now: number,
  mode: TimerMode,
  focusMinutes: number,
): {
  idle: boolean;
  countdown: boolean;
  text: string;
  value: number;
  title: string;
  accent: WidgetRole;
} {
  const idle = !timer || timer.status === "idle";
  const countdown = (idle ? mode : timer.mode) === "pomodoro";
  const full = focusMinutes * 60_000;
  const ms = idle ? (countdown ? full : 0) : timerValueMs(timer, now);
  const total = idle ? full : timer.totalMs;
  return {
    idle,
    countdown,
    text: clock(ms, countdown),
    value: total > 0 ? ms / total : 0,
    title: idle ? (countdown ? "Focus" : "Stopwatch") : timerTitle(timer),
    // Move red while focusing, Exercise green on a break: a hue per phase.
    accent: !idle && timer.phase !== "focus" ? "exercise" : "move",
  };
}

/** The words under the timer when it is not idle. */
export function timerCaption(timer: TimerState): string {
  if (timer.status === "ready") return `${timerTitle(timer)} is ready when you are.`;
  if (timer.status === "paused") return "Paused.";
  return timer.mode === "pomodoro"
    ? `${timer.preset.focus} minutes of focus, then ${timer.preset.rest} to rest.`
    : "Counting up.";
}

const DAY_INITIAL = ["S", "M", "T", "W", "T", "F", "S"];

/**
 * Seven days of finished focus as bar heights (0…1), today last. At least an
 * hour of headroom so one short session is not a full bar, and a day with any
 * focus keeps a sliver so it never reads as empty.
 */
export function weekWidget(days: FocusDay[], streak: number) {
  const top = Math.max(60, ...days.map((d) => d.minutes));
  const total = days.reduce((sum, d) => sum + d.minutes, 0);
  const today = days[days.length - 1];
  return {
    bars: days.map((d, i) => ({
      key: d.day,
      initial: DAY_INITIAL[new Date(`${d.day}T12:00:00`).getDay()],
      height: d.minutes > 0 ? Math.max(0.12, d.minutes / top) : 0,
      label: `${d.day}: ${d.sessions} sessions, ${d.minutes} minutes`,
      today: i === days.length - 1,
    })),
    total,
    totalWord: total > 0 ? `${formatMinutes(total)} this week` : "None this week",
    streakWord: streak > 0 ? "day streak" : "No streak yet",
    todaySessions: today?.sessions ?? 0,
  };
}

/** Which keep-awake choice is live, so it shows after the panel reopens. */
export function awakeChoice(awake: AwakeState | undefined): string {
  if (!awake?.active) return "off";
  return awake.minutes == null ? "always" : String(awake.minutes);
}

/** First non-empty line, without markdown's `#`: what a note is called. */
export function noteTitle(text: string): string {
  const line = text.split("\n").find((l) => l.trim() !== "") ?? "";
  return line.replace(/^#+\s*/, "").trim() || "Empty note";
}

/** The line after the title: the preview a Notes row carries. */
export function noteSnippet(text: string): string {
  const lines = text.split("\n").filter((l) => l.trim() !== "");
  return (lines[1] ?? "").replace(/^#+\s*/, "").trim();
}

/** When a note was written the way Notes says it: a time today, then Yesterday, then a date. */
export function noteWhen(unixSeconds: number, now: number): string {
  const at = new Date(unixSeconds * 1000);
  const today = new Date(now);
  const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((dayStart(today) - dayStart(at)) / 86_400_000);
  if (days === 0) return at.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  if (days === 1) return "Yesterday";
  return at.toLocaleDateString(undefined, { month: "numeric", day: "numeric", year: "2-digit" });
}
