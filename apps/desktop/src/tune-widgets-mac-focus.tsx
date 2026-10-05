import { useState, type ReactNode } from "react";
import type {
  Battery,
  FocusSnapshot,
  FocusStats,
  Note,
  PrivacyUse,
  SystemStats,
  TimerMode,
  TimerState,
} from "./lib/bindings";
import {
  AwakeBoard,
  NotesBoard,
  TimerBoard,
} from "./features/notch/views/widgets/Focus";
import {
  PowerBoard,
  PrivacyBoard,
  StatsBoard,
  WindowsBoard,
} from "./features/notch/views/widgets/Mac";

/**
 * Fixtures for the Mac and Focus widget views: each board with real-shaped
 * data, empty, and in its urgent or unusual states. Labelled as fixtures and
 * used only on the tune page; the app never shows them.
 */

const MIN = 60_000;
const noop = () => {};

const STATS: SystemStats = {
  cpu: 41,
  memoryUsedGb: 11,
  memoryTotalGb: 16,
  diskUsedGb: 212,
  diskTotalGb: 512,
  netDown: 1_400_000,
  netUp: 52_000,
  thermal: "nominal",
};

const BATTERY: Battery = { percent: 82, state: "discharging", minutes: 300, cycles: 120, health: 94 };

const timer = (now: number, over: Partial<TimerState> = {}): TimerState => ({
  mode: "pomodoro",
  phase: "focus",
  status: "running",
  round: 2,
  rounds: 4,
  preset: { focus: 25, rest: 5 },
  at: now + 18 * MIN + 42_000,
  heldMs: 0,
  totalMs: 25 * MIN,
  ...over,
});

const isoDay = (daysAgo: number) => {
  const d = new Date(Date.now() - daysAgo * 86_400_000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
};

const STATS_FOCUS: FocusStats = {
  streak: 5,
  week: [25, 50, 0, 75, 25, 100, 50].map((minutes, i) => ({
    day: isoDay(6 - i),
    sessions: minutes / 25,
    minutes,
  })),
};

const NOTES: Note[] = [
  { path: "a", title: "", date: Date.now() / 1000 - 3600, content: "Call prep: Maya\nOpen with the usage numbers" },
  { path: "b", title: "", date: Date.now() / 1000 - 86_400, content: "Grocery list\nOats, coffee, lemons, eggs" },
  { path: "c", title: "", date: Date.now() / 1000 - 9 * 86_400, content: "# Notch ideas\nWidgets are an option, not the default" },
];

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h3>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

function TimerFixture({ build, withStats = true }: { build: (now: number) => TimerState | undefined; withStats?: boolean }) {
  const [mode, setMode] = useState<TimerMode>("pomodoro");
  const [preset, setPreset] = useState("25-5");
  const now = Date.now();
  return (
    <TimerBoard
      timer={build(now)}
      now={now}
      stats={withStats ? STATS_FOCUS : null}
      mode={mode}
      onMode={setMode}
      preset={preset}
      onPreset={setPreset}
      onPrimary={noop}
      onSkip={noop}
      onStop={noop}
    />
  );
}

function AwakeFixture({ active, minutes, error = null }: { active: boolean; minutes: number | null; error?: string | null }) {
  const snap = {
    timer: timer(Date.now(), { status: "idle" }),
    awake: { active, minutes, until: minutes ? Date.now() + minutes * MIN : null },
  } as FocusSnapshot;
  return <AwakeBoard snap={snap} error={error} onChoose={noop} />;
}

function NotesFixture({ notes }: { notes: Note[] }) {
  const [text, setText] = useState("");
  return <NotesBoard text={text} notes={notes} openPath="a" now={Date.now()} onEdit={setText} onOpen={noop} onNew={() => setText("")} />;
}

const PRIVACY_ON: PrivacyUse = { mic: true, camera: true, micApps: ["Zoom"] };

export function MacFocusGallery() {
  return (
    <div className="grid max-w-[720px] gap-9 font-notch-w">
      <Block name="Mac · Stats" note="fixture: CPU, memory and disk as Move, Exercise, Stand; network and heat beside them">
        <StatsBoard stats={STATS} />
      </Block>
      <Block name="Mac · Stats, hot and reading" note="serious heat takes the gauge treatment and still says Hot; before the first read">
        <StatsBoard stats={{ ...STATS, cpu: 93, thermal: "serious" }} />
        <div className="h-4" />
        <StatsBoard stats={undefined} />
      </Block>
      <Block name="Mac · Power" note="fixture: discharging, with capacity and cycles and the alert switch on the tile">
        <PowerBoard battery={BATTERY} alerts onAlerts={noop} />
      </Block>
      <Block name="Mac · Power, charging, low and empty" note="charging stays green even when low; very low goes red with its word; a desktop Mac; unread">
        <PowerBoard battery={{ ...BATTERY, percent: 14, state: "charging", minutes: 95 }} alerts={false} onAlerts={noop} />
        <div className="h-4" />
        <PowerBoard battery={{ ...BATTERY, percent: 8, minutes: 24, cycles: null, health: null }} alerts onAlerts={noop} />
        <div className="h-4" />
        <PowerBoard battery={null} alerts={null} onAlerts={noop} />
        <div className="h-4" />
        <PowerBoard battery={undefined} alerts={null} onAlerts={noop} />
      </Block>
      <Block name="Mac · Privacy" note="fixture: mic and camera in use, then everything quiet, then unread">
        <PrivacyBoard use={PRIVACY_ON} watching onWatching={noop} />
        <div className="h-4" />
        <PrivacyBoard use={{ mic: false, camera: false, micApps: [] }} watching onWatching={noop} />
        <div className="h-4" />
        <PrivacyBoard use={undefined} watching={null} onWatching={noop} />
      </Block>
      <Block name="Mac · Windows" note="fixture: the snap grid; no window to snap; Accessibility not yet allowed">
        <WindowsBoard front={{ trusted: true, app: "Safari" }} said="Moved Safari to the left half" onSnap={noop} onOpenSettings={noop} />
        <div className="h-4" />
        <WindowsBoard front={{ trusted: true, app: null }} said="" onSnap={noop} onOpenSettings={noop} />
        <div className="h-4" />
        <WindowsBoard front={{ trusted: false, app: "Safari" }} said="" onSnap={noop} onOpenSettings={noop} />
      </Block>
      <Block name="Focus · Timer" note="fixture: running (drains), idle with the week, paused, a break, a stopwatch">
        <TimerFixture build={(now) => timer(now)} />
        <div className="h-4" />
        <TimerFixture build={() => undefined} />
        <div className="h-4" />
        <TimerFixture build={(now) => timer(now, { status: "paused", heldMs: 7 * MIN })} />
        <div className="h-4" />
        <TimerFixture build={(now) => timer(now, { phase: "shortBreak", totalMs: 5 * MIN, at: now + 3 * MIN })} withStats={false} />
        <div className="h-4" />
        <TimerFixture build={(now) => timer(now, { mode: "stopwatch", at: now - 65_000 })} />
      </Block>
      <Block name="Focus · Keep Awake" note="fixture: off, then on for two hours, then always, then a start that failed">
        <AwakeFixture active={false} minutes={null} />
        <div className="h-4" />
        <AwakeFixture active minutes={120} />
        <div className="h-4" />
        <AwakeFixture active minutes={null} />
        <div className="h-4" />
        <AwakeFixture active={false} minutes={null} error="caffeinate could not start" />
      </Block>
      <Block name="Focus · Notes" note="fixture: three notes and a note being written; then none yet">
        <NotesFixture notes={NOTES} />
        <div className="h-4" />
        <NotesFixture notes={[]} />
      </Block>
    </div>
  );
}
