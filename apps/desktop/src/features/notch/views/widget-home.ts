import type {
  AgentPermission,
  AgentSession,
  Battery,
  FocusSnapshot,
  Island,
} from "@/lib/bindings";
import type { WidgetRole } from "@/components/notch/widgets";
import { clock, timerValueMs } from "@/lib/focus";
import { batteryTone } from "./mac-format";

/**
 * What each Home widget says, worked out from the data the notch already has.
 * Apart from the components so the words and the ring values, which can
 * silently be wrong, are testable; the components only place them.
 */

/** The battery tile: the charge, what it is doing in a word, and the accent it earns. */
export function batteryWidget(b: Battery): {
  value: number;
  text: string;
  word: string;
  accent: WidgetRole;
} {
  const tone = batteryTone(b.percent, b.state);
  const word = { charging: "Charging", discharging: "On battery", full: "Full", pluggedIn: "Plugged in" }[b.state];
  return {
    value: b.percent / 100,
    text: String(b.percent),
    word: tone === "danger" ? "Very low" : tone === "warn" ? "Low" : word,
    // Healthy is green; going flat is amber and then red, always with its word.
    accent: tone === "danger" ? "move" : tone === "warn" ? "orange" : "ok",
  };
}

/**
 * The timer tile. A countdown is a ring draining (Fitness's Move ring while it
 * focuses, Exercise on a break); a stopwatch has no end to drain towards, so
 * its ring stays empty and the numerals carry it.
 */
export function timerWidget(
  snap: FocusSnapshot | null,
  now: number,
): { text: string; word: string; value: number; accent: WidgetRole; running: boolean; idle: boolean } {
  const t = snap?.timer;
  if (!t || t.status === "idle") {
    return { text: "–", word: "Timer", value: 0, accent: "move", running: false, idle: true };
  }
  const countdown = t.mode === "pomodoro";
  const ms = timerValueMs(t, now);
  const word =
    t.status === "paused"
      ? "Paused"
      : t.status === "ready"
        ? "Ready"
        : countdown
          ? t.phase === "focus"
            ? "Focus"
            : "Break"
          : "Stopwatch";
  return {
    text: clock(ms, countdown),
    word,
    value: countdown && t.totalMs > 0 ? ms / t.totalMs : 0,
    accent: t.phase === "focus" ? "move" : "exercise",
    running: t.status === "running",
    idle: false,
  };
}

/** The agents tile: what needs the user first, else what is working, else nothing. */
export function agentsWidget(
  sessions: AgentSession[],
  permissions: AgentPermission[],
): { count: number | null; word: string; live: boolean } {
  if (permissions.length > 0) return { count: permissions.length, word: "waiting for you", live: true };
  const busy = sessions.filter((s) => s.busy).length;
  if (busy > 0) return { count: busy, word: "running", live: true };
  return { count: null, word: sessions.length > 0 ? "all idle" : "no agents", live: false };
}

/**
 * The calendar tile without reading Calendar: today's date from the clock, and
 * the next event only if the island already carries one. Reading the calendar
 * is a script that can launch Calendar.app (`calendar_events`), which the
 * default tab must not do by opening; Today › Calendar does it on purpose.
 */
export function calendarWidget(
  islands: Island[],
  now: number,
  locale?: string,
): { weekday: string; day: string; next: { title: string; when: string } | null } {
  const date = new Date(now);
  const island = islands.find((i) => i.id === "calendar:next");
  return {
    weekday: date.toLocaleDateString(locale, { weekday: "long" }),
    day: String(date.getDate()),
    next: island ? { title: island.title, when: island.detail ?? "" } : null,
  };
}
