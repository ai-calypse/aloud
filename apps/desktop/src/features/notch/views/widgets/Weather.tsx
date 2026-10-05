import { useState, type FormEvent } from "react";
import { CloudMoonIcon, MoonIcon } from "lucide-react";
import { TextField } from "@/components/notch/field";
import { Numeral, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import type { Weather } from "@/lib/bindings";
import { degrees } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useWeather } from "../../useToday";
import { SKY } from "../sky";
import { Notice, Pill, QuietPill } from "./shared";
import { dayLabel, rangeBar, weekRange } from "./today-logic";
import { WeatherBackdrop } from "./weather-backdrop";
import { weatherSurface, type Glyph } from "./weather-surface";

/**
 * Today › Weather as a widget board: Apple's Weather widget (a tile that is the
 * sky, a Light temperature, and the days as low-to-high bars) beside a Place
 * tile that keeps the controls: change the city, go back to this
 * Mac's location, refresh.
 *
 * There is no hourly row: the weather read is five days, and an hour strip
 * would be made up.
 */
export function WidgetWeather() {
  const { data, error, loading, refresh, setCity, clearCity } = useWeather();
  return (
    <WeatherBoard
      data={data}
      error={error}
      loading={loading}
      onRefresh={() => void refresh()}
      onCity={(name) => setCity(name)}
      onLocate={() => void clearCity()}
    />
  );
}

export function WeatherBoard({
  data,
  error,
  loading,
  onRefresh,
  onCity,
  onLocate,
}: {
  data: Weather | null;
  error: string | null;
  loading: boolean;
  onRefresh: () => void;
  onCity: (name: string) => Promise<unknown>;
  onLocate: () => void;
}) {
  if (!data) {
    return (
      <WidgetBoard label="Weather">
        <Notice
          size="large"
          label="Weather"
          title={loading ? "Checking the sky" : "No weather right now"}
          action={!loading && <Pill onClick={onRefresh}>Try again</Pill>}
        >
          {error ?? "Weather comes from Open-Meteo, for where this Mac is."}
        </Notice>
      </WidgetBoard>
    );
  }
  return (
    <WidgetBoard label="Weather">
      <ForecastTile data={data} />
      <PlaceTile
        place={data.place}
        feelsLike={data.feelsLike}
        error={error}
        loading={loading}
        onRefresh={onRefresh}
        onCity={onCity}
        onLocate={onLocate}
      />
    </WidgetBoard>
  );
}

/** The sky's glyph: the sun is the one yellow thing, everything else is white. */
function SkyGlyph({ glyph, className }: { glyph: Glyph; className: string }) {
  const Icon = glyph === "moon" ? MoonIcon : glyph === "cloudMoon" ? CloudMoonIcon : SKY[glyph];
  const sun = glyph === "clear";
  return (
    <span
      data-w-role={sun ? "sun" : undefined}
      className={sun ? "text-widget-acc" : "text-widget-ink"}
    >
      <Icon aria-hidden className={className} strokeWidth={1.8} />
    </span>
  );
}

/** Weather, large: the blue gradient tile. */
export function ForecastTile({ data }: { data: Weather }) {
  const today = data.days[0];
  const range = weekRange(data.days);
  const surface = weatherSurface(data.sky, data.isDay);
  return (
    <WidgetFrame
      size="large"
      label="Weather"
      accent={surface.role}
      dim={surface.dim}
      ground="accent"
    >
      {/* The sky's deep shade to its accent, so white words pass AA at the bottom too. */}
      <div
        aria-hidden
        className="absolute inset-0 bg-linear-to-b from-widget-fill to-widget-acc/70"
      />
      {surface.backdrop && <WeatherBackdrop kind={surface.backdrop} />}
      <div className="relative flex h-full flex-col p-3.5">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="truncate text-notch-w-body leading-tight font-medium">
              {data.place}
            </div>
            <Numeral text={degrees(data.temperature)} size="l" />
          </div>
          <div className="shrink-0 text-right text-notch-w-title leading-snug font-medium">
            <SkyGlyph glyph={surface.glyph} className="ml-auto size-5" />
            <div>{data.summary}</div>
            {today && (
              <div className="text-widget-ink/85">
                H:{degrees(today.high)} L:{degrees(today.low)}
              </div>
            )}
          </div>
        </div>
        <div className="mt-2.5 text-notch-w-title leading-tight font-medium text-widget-ink/85">
          Feels like {degrees(data.feelsLike)}
        </div>
        <ul
          aria-label="Five-day forecast"
          className="m-0 mt-3 flex flex-1 list-none flex-col justify-between border-t border-widget-ink/25 p-0 pt-1.5"
        >
          {data.days.map((d, i) => {
            const bar = range
              ? rangeBar(d.low, d.high, range.low, range.high)
              : null;
            return (
              <li
                key={d.day}
                className="flex h-8 items-center gap-2 text-notch-w-title"
              >
                <span className="w-11 font-semibold">
                  {dayLabel(d.day, i === 0)}
                </span>
                <SkyGlyph glyph={d.sky} className="size-4.5" />
                <span className="w-8 text-right font-medium tabular-nums text-widget-ink/70">
                  {degrees(d.low)}
                </span>
                {/* The numbers carry the day; the bar only shows where it sits in the week. */}
                <span
                  aria-hidden
                  className="relative h-1 flex-1 rounded-full bg-widget-ink/20"
                >
                  {bar && (
                    <span
                      className="absolute inset-y-0 rounded-full bg-widget-ink"
                      style={{
                        left: `${bar.left * 100}%`,
                        width: `${bar.width * 100}%`,
                      }}
                    />
                  )}
                </span>
                <span className="w-8 font-semibold tabular-nums">
                  {degrees(d.high)}
                </span>
              </li>
            );
          })}
        </ul>
      </div>
    </WidgetFrame>
  );
}

/**
 * Weather, place: where the forecast is for, and the three things you can
 * do about it. The saved city, else wherever this connection is: a
 * coarse guess, so the city field is one tap away.
 */
export function PlaceTile({
  place,
  feelsLike,
  error,
  loading,
  onRefresh,
  onCity,
  onLocate,
}: {
  place: string;
  feelsLike: number | null;
  error: string | null;
  loading: boolean;
  onRefresh: () => void;
  onCity: (name: string) => Promise<unknown>;
  onLocate: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [city, setCity] = useState("");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!city.trim()) return;
    await onCity(city);
    setEditing(false);
    setCity("");
  };

  return (
    <WidgetFrame size="medium" label="Place">
      <div className="flex h-full flex-col justify-between p-3.5">
        <div>
          <div className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
            Forecast for
          </div>
          <div className="truncate text-notch-w-headline leading-tight font-semibold">
            {place}
          </div>
          <div className="text-notch-w-meta font-medium text-widget-ink-2">
            Feels like {degrees(feelsLike)}
          </div>
        </div>
        <div className="min-h-0">
          {error && (
            <p
              role="status"
              className={cn(
                "m-0 mb-1 truncate text-notch-w-meta text-notch-danger",
              )}
            >
              {error}
            </p>
          )}
          {editing ? (
            <form
              onSubmit={(e) => void submit(e)}
              className="flex items-center gap-1.5"
            >
              <TextField
                autoFocus
                aria-label="City"
                placeholder="City"
                value={city}
                onChange={(e) => setCity(e.target.value)}
                className="min-w-0 flex-1"
              />
              <Pill type="submit" disabled={loading}>
                Use city
              </Pill>
              <QuietPill onClick={() => setEditing(false)}>Cancel</QuietPill>
            </form>
          ) : (
            <div className="-ml-2.5 flex items-center">
              <QuietPill onClick={() => setEditing(true)}>
                Change city
              </QuietPill>
              <QuietPill onClick={onLocate}>Use my location</QuietPill>
              <QuietPill disabled={loading} onClick={onRefresh}>
                Refresh
              </QuietPill>
            </div>
          )}
        </div>
      </div>
    </WidgetFrame>
  );
}
