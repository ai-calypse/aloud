import { useCallback, useEffect, useState } from "react";
import {
  commands,
  type AgendaError,
  type Event as CalendarEvent,
  type Reminder,
  type Ticker,
  type Weather,
} from "@/lib/bindings";
import { usesFahrenheit } from "@/lib/today";

type Result<T, E> = { status: "ok"; data: T } | { status: "error"; error: E };

/** A command's `Result`, as state: the value, or the sentence that says why not. */
type Loaded<T> = { data: T | null; error: string | null; loading: boolean };

const fahrenheit = () => usesFahrenheit(navigator.language);

export function useWeather() {
  const [state, setState] = useState<Loaded<Weather>>({ data: null, error: null, loading: true });

  const apply = useCallback(
    (p: Promise<{ status: "ok"; data: Weather } | { status: "error"; error: string }>) => {
      setState((s) => ({ ...s, loading: true, error: null }));
      return p
        .then((r) =>
          setState((s) =>
            r.status === "ok"
              ? { data: r.data, error: null, loading: false }
              : // Keep what was on screen: stale weather beats a blank view.
                { data: s.data, error: r.error, loading: false },
          ),
        )
        .catch(() => setState((s) => ({ ...s, loading: false, error: "Weather needs the Aloud app." })));
    },
    [],
  );

  useEffect(() => {
    void apply(commands.weather(fahrenheit(), false));
  }, [apply]);

  return {
    ...state,
    refresh: () => apply(commands.weather(fahrenheit(), true)),
    setCity: (name: string) => apply(commands.setWeatherCity(name, fahrenheit())),
    clearCity: () => apply(commands.clearWeatherCity(fahrenheit())),
  };
}

const QUOTE_REFRESH_MS = 60_000;

/** The watchlist, refreshed each minute while the view is open and only then. */
export function useStocks() {
  const [tickers, setTickers] = useState<Ticker[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    const load = () =>
      commands
        .stockQuotes()
        .then((t) => live && setTickers(t))
        .catch(() => live && setError("The ticker needs the Aloud app."));
    void load();
    const id = window.setInterval(load, QUOTE_REFRESH_MS);
    return () => {
      live = false;
      window.clearInterval(id);
    };
  }, []);

  const change = async (p: Promise<{ status: "ok"; data: Ticker[] } | { status: "error"; error: string }>) => {
    const r = await p;
    if (r.status === "ok") {
      setTickers(r.data);
      setError(null);
    } else setError(r.error);
  };

  return {
    tickers,
    error,
    add: (symbol: string) => change(commands.addStock(symbol)),
    remove: (symbol: string) => change(commands.removeStock(symbol)),
  };
}

/** The Mac's clock, re-read every `every` ms while a view is open. */
export function useNow(every = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), every);
    return () => window.clearInterval(id);
  }, [every]);
  return now;
}

type Agenda<T> = { items: T[] | null; error: AgendaError | null };

/**
 * Calendar or Reminders as state. The first read can take seconds (it is a
 * script talking to another app), so `items` stays null until it lands and a
 * refresh keeps what was on screen.
 */
function useAgenda<T>(read: (refresh: boolean) => Promise<Result<T[], AgendaError>>) {
  const [state, setState] = useState<Agenda<T>>({ items: null, error: null });
  const load = useCallback(
    (refresh: boolean) =>
      read(refresh)
        .then((r) =>
          setState((s) =>
            r.status === "ok"
              ? { items: r.data, error: null }
              : { items: s.items, error: r.error },
          ),
        )
        .catch(() =>
          setState((s) => ({ ...s, error: { kind: "failed", message: "This needs the Aloud app." } })),
        ),
    [read],
  );
  useEffect(() => {
    void load(false);
  }, [load]);
  return { ...state, reload: () => load(true), setItems: (items: T[]) => setState({ items, error: null }) };
}

const readCalendar = (refresh: boolean) => commands.calendarEvents(refresh);
const readReminders = () => commands.remindersDue();

export const useCalendar = () => useAgenda<CalendarEvent>(readCalendar);
export const useReminders = () => useAgenda<Reminder>(readReminders);
