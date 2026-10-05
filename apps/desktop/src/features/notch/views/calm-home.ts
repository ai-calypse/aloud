import type {
  AgentPermission,
  AgentSession,
  Battery,
  Config,
  FocusSnapshot,
  Island,
  NowPlaying,
  Quota,
  Weather,
} from "@/lib/bindings";
import { pendingStep, type Run } from "@/lib/agent-steps";
import { clock, timerTitle, timerValueMs } from "@/lib/focus";
import { truncate } from "@/lib/format";
import { degrees } from "@/lib/today";
import { describeBinding } from "@/features/settings/binding-label";
import { shownWindows, percent } from "@/features/overlay/QuotaDashboard";
import { formatTime } from "../media";
import { runSummary, sessionState, usageWarning } from "./widgets/agents-logic";
import { agentsWidget, calendarWidget, timerWidget } from "./widget-home";

/**
 * Calm Home, worked out: which tile is how big and where, and what each tile
 * says. All pure, so the parts that can silently be wrong (a plan that does
 * not fill the grid, a word that disagrees with its colour) are testable.
 *
 * The weights, size sets, exact-22-cell plan and first-fit packing are
 * ported verbatim from docs/design/notch-personality.html (`TILES`,
 * `weights`, `pack`, `plan`), where they were tuned by eye on the Calm board.
 * The one change is `pack` backing up at a dead end, for states the prototype
 * never found a layout for.
 */

export type TileKey =
  | "media"
  | "timer"
  | "agents"
  | "battery"
  | "calendar"
  | "weather"
  | "dictate"
  | "note";
export type TileSize = "L" | "M" | "S";
/** The six tiles that grow and shrink; Dictate and New note are always 1×1. */
export type FlexKey = "media" | "timer" | "agents" | "battery" | "calendar" | "weather";
export type Placement = { id: TileKey; col: number; row: number; w: number; h: number; size: TileSize };
export type Plan = Record<TileKey, Placement>;

const COLS = 8;
const ROWS = 3;
/** The flex tiles fill 22 of the 24 cells; the two quick tiles take the last 2. */
const FLEX_AREA = 22;

/** [columns, rows] each flex tile can take. Order is the tie-break, as in the prototype. */
export const SIZES: Record<FlexKey, Partial<Record<TileSize, [number, number]>>> = {
  media: { L: [3, 2], S: [2, 1] },
  timer: { L: [2, 2], M: [2, 1], S: [1, 1] },
  agents: { L: [2, 2], M: [2, 1] },
  battery: { L: [1, 2], S: [1, 1] },
  calendar: { L: [4, 2], M: [3, 1], S: [2, 1] },
  weather: { L: [2, 2], M: [3, 1], S: [2, 1] },
};
const FLEX = Object.keys(SIZES) as FlexKey[];

/** What is live, as far as ranking cares. */
export type Live = {
  agents: boolean;
  timer: boolean;
  mediaPlaying: boolean;
  charging: boolean;
  /** `null` when this Mac has no battery or it has not been read. */
  batteryPercent: number | null;
};

/**
 * What the glance should be about right now. The closed notch ranks by the
 * same scores (battery pinned 100, agents 90, timer 70, media playing 60,
 * charging 40); the tiles read them tenths. An idle tile weighs 1, a
 * calendar 3.5 and the weather 1.5 whatever is live. Downloads (30) have no
 * tile, so they weigh on the pill only.
 */
export function weights(l: Live): Record<FlexKey, number> {
  const pinned = !l.charging && l.batteryPercent !== null && l.batteryPercent < 10;
  return {
    media: l.mediaPlaying ? 6 : 1,
    timer: l.timer ? 7 : 1,
    agents: l.agents ? 9 : 1,
    battery: pinned ? 10 : l.charging ? 4 : (l.batteryPercent ?? 100) < 30 ? 3 : 1.2,
    calendar: 3.5,
    weather: 1.5,
  };
}

type Item = { key: TileKey; size: TileSize; w: number; h: number };

/**
 * First-fit in reading order, backing up when an item has nowhere to go. The
 * prototype stopped at the first dead end; first-fit in weight order finds no
 * layout at all for some states (a live timer with live music, say), where
 * the prototype silently kept the previous layout. Backing up changes nothing
 * for a state first-fit solved, since its path is the first one tried.
 * ponytail: unbounded backtracking, fine for eight rectangles on 24 cells.
 */
function pack(items: Item[]): Plan | null {
  const taken = Array.from({ length: ROWS }, () => Array<boolean>(COLS).fill(false));
  const out: Partial<Plan> = {};
  const place = (i: number): boolean => {
    if (i === items.length) return true;
    const it = items[i];
    for (let r = 0; r + it.h <= ROWS; r++) {
      for (let c = 0; c + it.w <= COLS; c++) {
        const free = taken
          .slice(r, r + it.h)
          .every((row) => row.slice(c, c + it.w).every((cell) => !cell));
        if (!free) continue;
        for (let y = r; y < r + it.h; y++) taken[y].fill(true, c, c + it.w);
        out[it.key] = { id: it.key, col: c + 1, row: r + 1, w: it.w, h: it.h, size: it.size };
        if (place(i + 1)) return true;
        for (let y = r; y < r + it.h; y++) taken[y].fill(false, c, c + it.w);
      }
    }
    return false;
  };
  return place(0) ? (out as Plan) : null;
}

/**
 * Every combination of sizes whose areas add to exactly 22 cells, scored by
 * the area each tile gets times its weight; the best that packs wins. The
 * heaviest tile is placed first, so it lands top-left.
 *
 * ponytail: 216 combinations, a brute force per change of what is live. A
 * real solver if tiles grow past a dozen.
 */
export function plan(w: Record<FlexKey, number>): Plan {
  const options = FLEX.map((key) =>
    Object.entries(SIZES[key]).map(([size, [cw, ch]]) => ({
      key,
      size: size as TileSize,
      w: cw,
      h: ch,
    })),
  );
  let best: Plan | null = null;
  let bestScore = -1;
  const pick: Item[] = [];
  const recurse = (i: number, area: number) => {
    if (i === options.length) {
      if (area !== FLEX_AREA) return;
      const score = pick.reduce((s, p) => s + w[p.key as FlexKey] * p.w * p.h, 0);
      if (score <= bestScore) return;
      const ordered = [...pick]
        .sort((a, b) => w[b.key as FlexKey] - w[a.key as FlexKey] || b.w * b.h - a.w * a.h)
        .concat([
          { key: "dictate", size: "S", w: 1, h: 1 },
          { key: "note", size: "S", w: 1, h: 1 },
        ]);
      const packed = pack(ordered);
      if (packed) {
        best = packed;
        bestScore = score;
      }
      return;
    }
    for (const o of options[i]) {
      pick.push(o);
      recurse(i + 1, area + o.w * o.h);
      pick.pop();
    }
  };
  recurse(0, 0);
  // Unreachable: calm-home.test.ts packs every weight the app can produce.
  if (!best) throw new Error("calm home: no layout fills the grid");
  return best;
}

/** What is live, from the data the notch already holds. */
export function liveOf(
  sessions: AgentSession[],
  permissions: AgentPermission[],
  snap: FocusSnapshot | null,
  now: NowPlaying | null,
  battery: Battery | null | undefined,
): Live {
  return {
    agents: agentsWidget(sessions, permissions).live,
    timer: !!snap && snap.timer.status !== "idle",
    mediaPlaying: now?.state === "playing",
    charging: battery?.state === "charging",
    batteryPercent: battery?.percent ?? null,
  };
}

/* ---------------------------------------------------------------------------
 * What each tile says
 * ------------------------------------------------------------------------- */

/** The only three accents Calm has, each always beside a word. */
export type CalmState = "ok" | "caution" | "alert";

/** The battery's charge in a word: Good, Low (20% and under) or Critical (under 10%). */
export function calmBattery(b: Battery): {
  percent: number;
  word: string;
  state: CalmState;
  charging: boolean;
} {
  const onPower = b.state !== "discharging";
  const state: CalmState = onPower || b.percent > 20 ? "ok" : b.percent < 10 ? "alert" : "caution";
  const word =
    b.state === "charging"
      ? "Charging"
      : b.state === "full"
        ? "Full"
        : b.state === "pluggedIn"
          ? "Plugged in"
          : state === "alert"
            ? "Critical"
            : state === "caution"
              ? "Low"
              : "Good";
  return { percent: b.percent, word, state, charging: b.state === "charging" };
}

export type AgentRow = {
  id: string;
  name: string;
  detail: string;
  /** wait is the yellow dot, run the spinner, done the green tick, fail the red one. */
  state: "wait" | "run" | "done" | "fail" | "idle";
};

const RANK: Record<AgentRow["state"], number> = { wait: 0, fail: 1, run: 2, done: 3, idle: 4 };

/**
 * The agents tile: the coding sessions and the voice command, whatever needs
 * the user first. `names` is agentIdentity's AGENT_NAME, passed in so this
 * stays free of the overlay's images.
 */
export function calmAgents(
  sessions: AgentSession[],
  permissions: AgentPermission[],
  voice: Run | null,
  names: Record<string, string>,
): { rows: AgentRow[]; active: number; needs: number; word: string } {
  const asking = new Set(permissions.flatMap((p) => [p.id, p.sessionId]));
  const rows: AgentRow[] = sessions.map((s) => {
    const wait = asking.has(s.id);
    const { word } = sessionState(s, wait);
    const folder = s.cwd?.split(/[/\\]/).filter(Boolean).pop();
    const agent = names[s.agent] ?? s.agent;
    return {
      id: s.id,
      name: folder ? `${agent} · ${folder}` : agent,
      detail: word,
      state: wait ? "wait" : s.busy || s.tool ? "run" : "idle",
    };
  });
  if (voice) {
    const r = runSummary(voice);
    const state: AgentRow["state"] =
      r.word === "Failed"
        ? "fail"
        : r.word === "Done"
          ? "done"
          : r.word === "Working"
            ? "run"
            : "wait";
    rows.push({ id: "voice", name: `Voice · ${truncate(voice.heard, 24)}`, detail: r.word, state });
  }
  rows.sort((a, b) => RANK[a.state] - RANK[b.state]);

  const needs = permissions.length + (voice && pendingStep(voice) ? 1 : 0);
  const active = rows.filter((r) => r.state === "wait" || r.state === "run").length;
  const busy = rows.filter((r) => r.state === "run").length;
  const word = needs > 0 ? `${needs} needs you` : busy > 0 ? `${busy} running` : "all idle";
  return { rows, active: active || rows.length, needs, word };
}

/**
 * The plan-usage meter inside the agents tile, from the fullest window the
 * plan reports. Thresholds are the Usage view's (70 amber, 90 red), so the
 * two never disagree about the same number.
 */
export function calmUsage(
  quota: Quota | null,
): { percent: number; word: string | null; state: CalmState | undefined } | null {
  const windows = shownWindows(quota);
  if (windows.length === 0) return null;
  const pct = Math.round(Math.max(...windows.map(([, w]) => percent(w))));
  const word = usageWarning(pct);
  return { percent: pct, word, state: pct >= 90 ? "alert" : pct >= 70 ? "caution" : undefined };
}

/** The timer tile: the widget's words, plus what Calm adds (the session count, the button). */
export function calmTimer(snap: FocusSnapshot | null, now: number) {
  const t = timerWidget(snap, now);
  const timer = snap?.timer;
  const countdown = timer?.mode === "pomodoro";
  const ms = timer ? timerValueMs(timer, now) : 0;
  return {
    ...t,
    title: timer && !t.idle ? timerTitle(timer) : "Timer",
    /** "2 of 4": a countdown's place in its set; a stopwatch has none. */
    chip: timer && countdown && !t.idle ? `${timer.round} of ${timer.rounds}` : null,
    rounds: countdown && timer ? { done: timer.round, of: timer.rounds } : null,
    clock: timer ? clock(ms, countdown) : "–",
    /** Whole minutes for the 1×1: a countdown rounds up, so 0 only shows when it is over. */
    minutes: String(countdown ? Math.ceil(ms / 60_000) : Math.floor(ms / 60_000)),
    button:
      timer?.status === "running"
        ? { label: "Pause", action: "focus.pause" }
        : timer && timer.status !== "idle"
          ? { label: timer.status === "ready" ? "Start" : "Resume", action: "focus.resume" }
          : null,
  };
}

/** The now-playing tile. `positionMs` is already advanced to this instant. */
export function calmMedia(now: NowPlaying, positionMs: number) {
  const known = now.durationMs > 0;
  return {
    title: now.title,
    artist: now.artist,
    playing: now.state === "playing",
    /** A web video can have no length, and then there is nothing to scrub. */
    scrub: known,
    progress: known ? Math.min(1, positionMs / now.durationMs) : 0,
    position: formatTime(positionMs),
    left: known ? `-${formatTime(now.durationMs - positionMs)}` : "",
  };
}

/** "in 25 min" and "now" are the next-event island's whole vocabulary (aloud_agenda::island). */
export function minutesUntil(when: string): number | null {
  if (when === "now") return 0;
  const m = /^in (\d+) min$/.exec(when);
  return m ? Number(m[1]) : null;
}

/** The island appears within an hour of the event; the countdown fills over that hour. */
const LEAD_MINUTES = 60;

export type MonthCell = { day: number; today: boolean; weekend: boolean } | null;

/**
 * The month as a Monday-first grid with leading blanks, English initials.
 * ponytail: Monday-first and English headers whatever the locale; Intl's
 * weekInfo is the upgrade path once WebKit ships it everywhere.
 */
export function monthGrid(now: number, locale?: string) {
  const d = new Date(now);
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const days = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
  const blanks = (first.getDay() + 6) % 7;
  const cells: MonthCell[] = Array.from({ length: blanks }, () => null);
  for (let day = 1; day <= days; day++) {
    cells.push({ day, today: day === d.getDate(), weekend: (blanks + day - 1) % 7 > 4 });
  }
  return {
    month: d.toLocaleDateString(locale, { month: "short" }),
    initials: ["M", "T", "W", "T", "F", "S", "S"],
    cells,
  };
}

/**
 * The calendar tile without reading Calendar (see `calendarWidget`): the date
 * from the clock, and the next event only if the island already carries one,
 * said as minutes because that is all the island knows.
 */
export function calmCalendar(islands: Island[], now: number, locale?: string) {
  const c = calendarWidget(islands, now, locale);
  const minutes = c.next ? minutesUntil(c.next.when) : null;
  return {
    ...c,
    minutes,
    /** 0…1 across the hour before the event. */
    progress: minutes === null ? 0 : 1 - Math.min(minutes, LEAD_MINUTES) / LEAD_MINUTES,
    month: monthGrid(now, locale),
  };
}

/** The weather tile; the five days' highs are the only series Rust has, so that is the spark. */
export function calmWeather(w: Weather) {
  const today = w.days[0];
  return {
    temp: degrees(w.temperature),
    summary: w.summary,
    range: today ? `H ${degrees(today.high)} L ${degrees(today.low)}` : null,
    highs: w.days.map((d) => d.high).filter((h): h is number => h !== null),
  };
}

/** What the Dictate tile says to do: the user's own hotkey, not a hard-coded one. */
export function dictateHint(hotkeys: Config | undefined): string {
  const hold = hotkeys?.hold;
  if (hold && hold.kind !== "disabled") return `Hold ${describeBinding(hold)}`;
  const toggle = hotkeys?.toggle;
  if (toggle && toggle.kind !== "disabled") return `Press ${describeBinding(toggle)}`;
  return "No hotkey set";
}
