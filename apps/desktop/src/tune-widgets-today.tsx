import type { ReactNode } from "react";
import type { AgendaError, Event as CalendarEvent, Reminder, Ticker, Weather } from "./lib/bindings";
import { CalendarBoard } from "./features/notch/views/widgets/Calendar";
import { RemindersBoard } from "./features/notch/views/widgets/Reminders";
import { StocksBoard } from "./features/notch/views/widgets/Stocks";
import { WeatherBoard } from "./features/notch/views/widgets/Weather";

/**
 * The Today widgets' fixtures for the tune page: each subsection with real-
 * shaped data, and again as the states a Mac can really be in (not allowed,
 * still reading, nothing there). Fixtures are labelled as such and live only
 * here; the app never shows them.
 */

const HOUR = 3_600_000;
const NOON_OF_NOW = new Date(2026, 9, 4, 14, 5).getTime();
const DAY = new Date(NOON_OF_NOW).setHours(0, 0, 0, 0);
const at = (day: 0 | 1, h: number, m = 0) => DAY + day * 24 * HOUR + (h * 60 + m) * 60_000;

let n = 0;
const ev = (
  title: string,
  from: number,
  to: number,
  extra: Partial<CalendarEvent> = {},
): CalendarEvent => ({
  id: `fixture-${n++}`,
  title,
  start: from,
  end: to,
  allDay: false,
  location: null,
  joinUrl: null,
  calendar: "Work",
  ...extra,
});

const EVENTS: CalendarEvent[] = [
  ev("Ravi's Birthday", at(1, 0), at(1, 24), { allDay: true, calendar: "Birthdays" }),
  ev("Design sync", at(0, 14, 30), at(0, 15), { joinUrl: "https://meet.example/design" }),
  ev("1:1 with Maya", at(0, 16), at(0, 16, 30), { calendar: "Home" }),
  ev("Roadmap review", at(0, 16), at(0, 17, 30), { calendar: "Team" }),
  ev("Dinner with Sam", at(0, 19), at(0, 21), { calendar: "Home", location: "Nopa" }),
  ev("Late call", at(0, 21, 30), at(0, 22), { calendar: "Team" }),
  ev("Evening class", at(0, 22, 30), at(0, 23, 30), { calendar: "Home" }),
  ev("Morning swim", at(1, 9), at(1, 9, 45), { calendar: "Home", location: "Home" }),
  ev("Design review", at(1, 10), at(1, 13), { joinUrl: "https://meet.example/review" }),
  ev("Team lunch", at(1, 13, 30), at(1, 14, 30), { calendar: "Team" }),
  ev("Regroup", at(1, 15), at(1, 16)),
  ev("Dinner out", at(1, 19), at(1, 21), { calendar: "Home" }),
];

const REMINDERS: Reminder[] = [
  { id: "r1", title: "Send standup notes", due: at(0, 9), allDay: false, list: "Work" },
  { id: "r2", title: "Review Rigo's draft", due: at(0, 15), allDay: false, list: "Work" },
  { id: "r3", title: "Book club prep", due: DAY, allDay: true, list: "Reminders" },
  { id: "r4", title: "Pick up arts & crafts supplies", due: at(0, 17), allDay: false, list: "Home" },
  { id: "r5", title: "Cancel membership", due: DAY - 24 * HOUR, allDay: true, list: "Reminders" },
  { id: "r6", title: "Schedule car maintenance", due: at(0, 18), allDay: false, list: "Home" },
  { id: "r7", title: "Check spare tire", due: at(0, 18, 30), allDay: false, list: "Home" },
  { id: "r8", title: "Hike with Darla", due: at(0, 19), allDay: false, list: "Home" },
];

const WEATHER: Weather = {
  place: "Oakland",
  temperature: 21,
  feelsLike: 20,
  sky: "partlyCloudy",
  summary: "Partly cloudy",
  isDay: true,
  days: [
    { day: "2026-10-04", sky: "partlyCloudy", high: 24, low: 15 },
    { day: "2026-10-05", sky: "cloudy", high: 20, low: 13 },
    { day: "2026-10-06", sky: "rain", high: 18, low: 12 },
    { day: "2026-10-07", sky: "clear", high: 26, low: 14 },
    { day: "2026-10-08", sky: "clear", high: 28, low: 16 },
  ],
};

const TICKERS: Ticker[] = [
  { symbol: "AAPL", name: "Apple Inc.", price: 232.14, changePct: 0.85, state: "open", delayed: false, error: null, currency: "USD" },
  { symbol: "^GSPC", name: "S&P 500", price: 5712.69, changePct: -1.2, state: "closed", delayed: false, error: null, currency: "USD" },
  { symbol: "BTC-USD", name: "Bitcoin USD", price: 85809.2, changePct: 0.02, state: "open", delayed: true, error: null, currency: "USD" },
  { symbol: "NOPE", name: "", price: null, changePct: null, state: null, delayed: false, error: "No quote for this symbol", currency: null },
];

const NOT_PERMITTED: AgendaError = { kind: "notPermitted", app: "Calendar" };
const noop = () => {};

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

export function TodayWidgetGallery() {
  const calendar = (items: CalendarEvent[] | null, error: AgendaError | null = null) => (
    <CalendarBoard items={items} error={error} now={NOON_OF_NOW} onReload={noop} onJoin={noop} />
  );
  const reminders = (items: Reminder[] | null, error: AgendaError | null = null) => (
    <RemindersBoard items={items} error={error} now={NOON_OF_NOW} ticking={new Set(["r2"])} failed={null} onTick={noop} onReload={noop} />
  );
  return (
    <div data-notch-style="widgets" className="grid max-w-[720px] gap-9 font-notch-w">
      <Block name="Today · Calendar" note="fixtures: a Sunday at 2:05 PM with an overlap, a video link, an all-day event and hidden events">
        {calendar(EVENTS)}
      </Block>
      <Block name="Today · Calendar, states" note="not allowed, still reading, and a quiet day">
        <div className="grid gap-6">
          {calendar(null, NOT_PERMITTED)}
          {calendar(null)}
          {calendar([])}
        </div>
      </Block>
      <Block name="Today · Reminders" note="fixtures: eight due, one slipped, one mid-tick">
        {reminders(REMINDERS)}
      </Block>
      <Block name="Today · Reminders, states" note="nothing due, not allowed">
        <div className="grid gap-6">
          {reminders([])}
          {reminders(null, { kind: "notPermitted", app: "Reminders" })}
        </div>
      </Block>
      <Block name="Today · Weather" note="fixtures: five days, partly cloudy">
        <WeatherBoard data={WEATHER} error={null} loading={false} onRefresh={noop} onCity={async () => {}} onLocate={noop} />
      </Block>
      <Block name="Today · Weather, states" note="no data yet, and a refresh that failed over stale data">
        <div className="grid gap-6">
          <WeatherBoard data={null} error="Weather needs the Aloud app." loading={false} onRefresh={noop} onCity={async () => {}} onLocate={noop} />
          <WeatherBoard data={{ ...WEATHER, sky: "clear", summary: "Clear", isDay: false }} error="Could not reach Open-Meteo" loading={false} onRefresh={noop} onCity={async () => {}} onLocate={noop} />
        </div>
      </Block>
      <Block name="Today · Stocks" note="fixtures: up, down, a delayed pair and a symbol with no quote">
        <StocksBoard tickers={TICKERS} error={null} onAdd={async () => {}} onRemove={noop} />
      </Block>
      <Block name="Today · Stocks, states" note="nothing followed, and a refused symbol">
        <div className="grid gap-6">
          <StocksBoard tickers={[]} error={null} onAdd={async () => {}} onRemove={noop} />
          <StocksBoard tickers={TICKERS.slice(0, 2)} error="Unknown symbol: ZZZZ" onAdd={async () => {}} onRemove={noop} />
        </div>
      </Block>
    </div>
  );
}
