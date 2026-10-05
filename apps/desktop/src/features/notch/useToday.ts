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
import { remember, remembered, revalidate } from "./swr";

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

/** A stable default: a fresh function per render would re-run the load effect forever. */
const noPeek = () => Promise.resolve(null);

type Agenda<T> = { items: T[] | null; error: AgendaError | null };

/**
 * Calendar or Reminders as state, stale-while-revalidate: what was last seen
 * (this webview's memory, then Rust's file from the last run through `peek`)
 * is the first frame, and the real read, which is a script or EventKit
 * talking to another app, only corrects it. `items` stays null, and the view
 * shows its loading words, only when there has never been anything to show.
 * A failed read keeps the rows on screen and reports beside them.
 */
function useAgenda<T>(
  key: string,
  read: (refresh: boolean) => Promise<Result<T[], AgendaError>>,
  peek: () => Promise<T[] | null> = noPeek,
) {
  const [state, setState] = useState<Agenda<T>>(() => ({ items: remembered<T[]>(key), error: null }));
  const apply = useCallback(
    (items: T[]) => {
      remember(key, items);
      setState({ items, error: null });
    },
    [key],
  );
  const load = useCallback(
    (refresh: boolean) =>
      revalidate<T[], AgendaError>({
        peek,
        fresh: () =>
          read(refresh).catch(
            (): Result<T[], AgendaError> => ({ status: "error", error: { kind: "failed", message: "This needs the Aloud app." } }),
          ),
        onItems: apply,
        onError: (error) => setState((s) => ({ items: s.items, error })),
      }),
    [read, peek, apply],
  );
  useEffect(() => {
    void load(false);
  }, [load]);
  return { ...state, reload: () => load(true), setItems: apply };
}

const readCalendar = (refresh: boolean) => commands.calendarEvents(refresh);
const readReminders = () => commands.remindersDue();
const peekReminders = () => commands.remindersCached();

export const useCalendar = () => useAgenda<CalendarEvent>("calendar", readCalendar);
export const useReminders = () => useAgenda<Reminder>("reminders", readReminders, peekReminders);

/**
 * One date's events for the month card. `null` selects nothing to read (today
 * and tomorrow come from the board's own read). A date already visited shows
 * at once and is re-read behind it.
 */
export function useDayEvents(day: number | null) {
  const key = `day:${day}`;
  const read = useCallback(() => commands.calendarDay(day), [day]);
  const [state, setState] = useState<Agenda<CalendarEvent>>({ items: null, error: null });
  useEffect(() => {
    if (day === null) return;
    let live = true;
    setState({ items: remembered<CalendarEvent[]>(key), error: null });
    void revalidate<CalendarEvent[], AgendaError>({
      peek: () => Promise.resolve(null),
      fresh: () =>
        read().catch((): Result<CalendarEvent[], AgendaError> => ({
          status: "error",
          error: { kind: "failed", message: "This needs the Aloud app." },
        })),
      onItems: (items) => {
        remember(key, items);
        if (live) setState({ items, error: null });
      },
      onError: (error) => live && setState((s) => ({ items: s.items, error })),
    });
    return () => {
      live = false;
    };
  }, [day, key, read]);
  return state;
}
