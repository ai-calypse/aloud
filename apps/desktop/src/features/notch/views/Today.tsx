import { useState, type FormEvent } from "react";
import {
  CloudDrizzleIcon,
  CloudFogIcon,
  CloudIcon,
  CloudLightningIcon,
  CloudRainIcon,
  CloudSnowIcon,
  CloudSunIcon,
  MoonIcon,
  SunIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { TextField } from "@/components/notch/field";
import { Empty, Lead, List, Row, Value } from "@/components/notch/list";
import type { Sky } from "@/lib/bindings";
import { degrees, marketLabel, percent, price } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useStocks, useWeather } from "../useToday";

const SKY: Record<Sky, LucideIcon> = {
  clear: SunIcon,
  partlyCloudy: CloudSunIcon,
  cloudy: CloudIcon,
  fog: CloudFogIcon,
  drizzle: CloudDrizzleIcon,
  rain: CloudRainIcon,
  snow: CloudSnowIcon,
  storm: CloudLightningIcon,
};

/** The day's name, from the place's own date so a far timezone cannot shift it. */
function dayName(day: string, first: boolean): string {
  if (first) return "Today";
  return new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { weekday: "short" });
}

/**
 * Today › Weather: Open-Meteo through Rust. The place is the saved city, else
 * wherever this connection is — a coarse guess, so the city field is one tap
 * away rather than buried in settings.
 */
export function TodayWeather() {
  const { data, error, loading, refresh, setCity, clearCity } = useWeather();
  const [editing, setEditing] = useState(false);
  const [city, setCityText] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!city.trim()) return;
    await setCity(city);
    setEditing(false);
    setCityText("");
  };

  if (!data) {
    return (
      <Empty
        title={loading ? "Checking the sky" : "No weather right now"}
        action={
          !loading && (
            <Button variant="notch" size="notch" onClick={() => void refresh()}>
              Try again
            </Button>
          )
        }
      >
        {error ?? "Weather comes from Open-Meteo, for where this Mac is."}
      </Empty>
    );
  }

  const Now = data.sky === "clear" && !data.isDay ? MoonIcon : SKY[data.sky];
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start gap-8">
        <div className="flex shrink-0 items-center gap-3">
          <Now aria-hidden className="size-9 text-notch-ink-2" strokeWidth={1.5} />
          <Lead sub={`${data.summary} · ${data.place}`}>{degrees(data.temperature)}</Lead>
        </div>
        <ul className="m-0 flex min-w-0 flex-1 list-none justify-between p-0" aria-label="Five-day forecast">
          {data.days.map((d, i) => {
            const Icon = SKY[d.sky];
            return (
              <li key={d.day} className="flex flex-col items-center gap-1 text-notch-meta">
                <span className="text-notch-ink-2">{dayName(d.day, i === 0)}</span>
                <Icon aria-label={d.sky} className="size-4 text-notch-ink-2" strokeWidth={1.6} />
                <span className="font-notch-mono tabular-nums text-notch-ink">{degrees(d.high)}</span>
                <span className="font-notch-mono tabular-nums text-notch-ink-3">{degrees(d.low)}</span>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="flex items-center gap-2 text-notch-meta text-notch-ink-3">
        {editing ? (
          <form onSubmit={(e) => void submit(e)} className="flex flex-1 items-center gap-1.5">
            <TextField
              autoFocus
              aria-label="City"
              placeholder="City"
              value={city}
              onChange={(e) => setCityText(e.target.value)}
              className="max-w-[220px]"
            />
            <Button variant="notch" size="notch" type="submit" disabled={loading}>
              Use city
            </Button>
            <Button variant="notch-quiet" size="notch" type="button" onClick={() => setEditing(false)}>
              Cancel
            </Button>
          </form>
        ) : (
          <>
            <span>Feels like {degrees(data.feelsLike)}</span>
            <span aria-hidden>·</span>
            <Button variant="notch-quiet" size="notch" onClick={() => setEditing(true)}>
              Change city
            </Button>
            <Button variant="notch-quiet" size="notch" onClick={() => void clearCity()}>
              Use my location
            </Button>
            <Button variant="notch-quiet" size="notch" disabled={loading} onClick={() => void refresh()}>
              Refresh
            </Button>
          </>
        )}
      </div>
      {error && <p className="m-0 text-notch-meta text-notch-danger">{error}</p>}
    </div>
  );
}

/**
 * Today › Stocks: the symbols you follow. Quotes are fetched while this view
 * is open, each marked with what its market is doing, so a closed market's
 * last price is never read as live.
 */
export function TodayStocks() {
  const { tickers, error, add, remove } = useStocks();
  const [symbol, setSymbol] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!symbol.trim()) return;
    await add(symbol);
    setSymbol("");
  };

  const form = (
    <form onSubmit={(e) => void submit(e)} className="mt-2 flex items-center gap-1.5">
      <TextField
        aria-label="Ticker symbol"
        placeholder="AAPL, ^GSPC, BTC-USD"
        value={symbol}
        onChange={(e) => setSymbol(e.target.value)}
        className="max-w-[220px]"
      />
      <Button variant="notch" size="notch" type="submit">
        Add
      </Button>
    </form>
  );

  return (
    <div className="flex flex-col">
      {tickers && tickers.length > 0 ? (
        <List label="Stocks">
          {tickers.map((t) => {
            const label = marketLabel(t);
            const move = t.changePct;
            return (
              <Row
                key={t.symbol}
                title={t.symbol}
                subtitle={t.error ?? [t.name, label].filter(Boolean).join(" · ")}
                trailing={
                  <div className="flex items-center gap-2">
                    {t.price != null && (
                      <Value className="text-notch-ink">
                        {price(t.price)}
                        {move != null && (
                          <span
                            className={cn(
                              "ml-2",
                              move > 0.05 && "text-notch-ok",
                              move < -0.05 && "text-notch-danger",
                            )}
                          >
                            {percent(move)}
                          </span>
                        )}
                      </Value>
                    )}
                    <Button
                      variant="notch-quiet"
                      size="notch"
                      aria-label={`Stop following ${t.symbol}`}
                      onClick={() => void remove(t.symbol)}
                    >
                      Remove
                    </Button>
                  </div>
                }
              />
            );
          })}
        </List>
      ) : (
        tickers && (
          <Empty title="Nothing followed yet">
            Add a symbol to see its price here: a stock, an index or a pair like BTC-USD.
          </Empty>
        )
      )}
      {tickers && form}
      {error && <p className="m-0 mt-2 text-notch-meta text-notch-danger">{error}</p>}
    </div>
  );
}
