import { useCallback, useEffect, useState } from "react";
import { commands, type Ticker, type Weather } from "@/lib/bindings";
import { usesFahrenheit } from "@/lib/today";

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
