import { useState, type FormEvent } from "react";
import {
  CheckIcon,
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
import { Button, NOTCH_FOCUS } from "@/components/ui/button";
import { TextField } from "@/components/notch/field";
import { Empty, Label, Lead, List, Row, Value } from "@/components/notch/list";
import { commands, type AgendaError, type Event as CalendarEvent, type Reminder, type Sky } from "@/lib/bindings";
import { degrees, dueText, marketLabel, nextEvent, overdue, percent, price, timeOfDay, untilText } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useCalendar, useNow, useReminders, useStocks, useWeather } from "../useToday";

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

/**
 * The view for a failed Calendar or Reminders read. A missing Automation
 * grant has one button, the System Settings flow; anything else says what
 * happened and offers a retry.
 */
function AgendaProblem({
  error,
  app,
  permission,
  retry,
}: {
  error: AgendaError;
  app: string;
  permission: "calendar" | "reminders";
  retry: () => void;
}) {
  if (error.kind === "notPermitted") {
    return (
      <Empty
        title={`Allow Aloud to read ${app}`}
        action={
          <Button variant="notch" size="notch" onClick={() => void commands.openPermissionSettings(permission)}>
            Allow access
          </Button>
        }
      >
        {permission === "calendar"
          ? "macOS asks once. Aloud only reads your events; it never adds or changes one."
          : "macOS asks once. Aloud reads what is due, and ticking one off is the only change it makes."}{" "}
        Come back here after answering.
      </Empty>
    );
  }
  return (
    <Empty
      title={`Could not read ${app}`}
      action={
        <Button variant="notch" size="notch" onClick={retry}>
          Try again
        </Button>
      }
    >
      {error.message}
    </Empty>
  );
}

/**
 * Today › Calendar: what is left of today and tomorrow, from the calendars
 * you can edit. The next event with a video link gets the one inverted button.
 */
export function TodayCalendar() {
  const { items, error, reload } = useCalendar();
  const now = useNow();

  if (!items) {
    return error ? (
      <AgendaProblem error={error} app="Calendar" permission="calendar" retry={() => void reload()} />
    ) : (
      <Empty title="Reading your calendar">The first read opens Calendar and can take a few seconds.</Empty>
    );
  }

  const live = items.filter((e) => e.end > now);
  const midnight = new Date(now).setHours(24, 0, 0, 0);
  const today = live.filter((e) => e.start < midnight);
  const tomorrow = live.filter((e) => e.start >= midnight);
  // What the island is showing too.
  const next = nextEvent(live, now);

  if (live.length === 0) {
    return (
      <Empty
        title="Nothing else on today or tomorrow"
        action={
          <Button variant="notch-quiet" size="notch" className="-ml-2.5" onClick={() => void reload()}>
            Refresh
          </Button>
        }
      >
        Events from the calendars you can edit show up here. Holiday and subscribed calendars are left out.
      </Empty>
    );
  }

  const group = (title: string, events: CalendarEvent[]) =>
    events.length > 0 && (
      <div key={title}>
        <Label>{title}</Label>
        <List label={title}>
          {events.map((e) => (
            <Row
              key={e.id}
              lead={
                <Value className={cn("w-[68px]", e.id === next?.id && "text-notch-ink")}>
                  {e.allDay ? "All day" : timeOfDay(e.start)}
                </Value>
              }
              title={e.title}
              subtitle={
                [e.id === next?.id ? untilText(e.start, e.end, now) : null, e.location ?? e.calendar]
                  .filter(Boolean)
                  .join(" · ")
              }
              trailing={
                e.joinUrl && (
                  <Button
                    variant={e.id === next?.id ? "notch" : "notch-quiet"}
                    size="notch"
                    onClick={() => void commands.openMeetingLink(e.joinUrl!)}
                  >
                    Join
                  </Button>
                )
              }
            />
          ))}
        </List>
      </div>
    );

  return (
    <div className="flex flex-col gap-3">
      {group("Today", today)}
      {group("Tomorrow", tomorrow)}
      {error && <p className="m-0 text-notch-meta text-notch-danger">Could not refresh: {error.kind === "failed" ? error.message : "access was refused"}</p>}
    </div>
  );
}

/**
 * Today › Reminders: due today or overdue, soonest first. The circle ticks one
 * off in Reminders itself — the only thing in Today that writes to your
 * account.
 */
export function TodayReminders() {
  const { items, error, reload, setItems } = useReminders();
  const now = useNow();
  const [ticking, setTicking] = useState<Set<string>>(new Set());
  const [failed, setFailed] = useState<string | null>(null);

  const tick = async (r: Reminder) => {
    setFailed(null);
    setTicking((s) => new Set(s).add(r.id));
    const res = await commands.completeReminder(r.id);
    setTicking((s) => {
      const next = new Set(s);
      next.delete(r.id);
      return next;
    });
    if (res.status === "ok") setItems((items ?? []).filter((x) => x.id !== r.id));
    else setFailed(res.error.kind === "failed" ? res.error.message : "Aloud is not allowed to change Reminders.");
  };

  if (!items) {
    return error ? (
      <AgendaProblem error={error} app="Reminders" permission="reminders" retry={() => void reload()} />
    ) : (
      <Empty title="Reading your reminders">The first read opens Reminders.</Empty>
    );
  }

  if (items.length === 0) {
    return (
      <Empty
        title="Nothing due"
        action={
          <Button variant="notch-quiet" size="notch" className="-ml-2.5" onClick={() => void reload()}>
            Refresh
          </Button>
        }
      >
        Reminders due today, and any that slipped past, show up here.
      </Empty>
    );
  }

  return (
    <div>
      <List label="Reminders due">
        {items.map((r) => {
          const done = ticking.has(r.id);
          const late = overdue(r.due, r.allDay, now);
          return (
            <Row
              key={r.id}
              lead={
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={done}
                  aria-label={`Mark done: ${r.title}`}
                  disabled={done}
                  onClick={() => void tick(r)}
                  className={cn(
                    "grid size-[18px] shrink-0 place-items-center rounded-full border border-notch-ink-3 outline-none",
                    "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:border-notch-ink",
                    done && "border-notch-ink bg-notch-ink text-notch-primary-ink",
                    NOTCH_FOCUS,
                  )}
                >
                  {done && <CheckIcon aria-hidden className="size-3" strokeWidth={3} />}
                </button>
              }
              title={r.title}
              subtitle={
                <span className={cn(late && "text-notch-warn")}>
                  {[dueText(r.due, r.allDay, now), r.list].filter(Boolean).join(" · ")}
                </span>
              }
            />
          );
        })}
      </List>
      {failed && <p className="m-0 mt-2 text-notch-meta text-notch-danger">{failed}</p>}
    </div>
  );
}
