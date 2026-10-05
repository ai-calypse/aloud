import { useState, type FormEvent } from "react";
import { XIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { TextField } from "@/components/notch/field";
import { WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import type { Ticker } from "@/lib/bindings";
import { marketLabel, percent, price } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useStocks } from "../../useToday";
import { Notice, Pill } from "./shared";
import { dayChart, moveOf } from "./today-logic";

/**
 * Today › Stocks as a widget board: Apple's Stocks list (symbol and name on
 * the left, price and a green or red change on the right) and a tile to follow
 * another symbol. Quotes are fetched while this is open, each
 * marked with what its market is doing so a closed market's last price is
 * never read as live. Each row carries the day's line, from the same quote
 * call, so the chart refreshes exactly when the price does.
 *
 * ponytail: today only. A 5-day or 1-month view is another Yahoo call per
 * symbol (a different range and interval), which would double the traffic the
 * minute-refresh makes; add it as an on-demand fetch behind a toggle.
 */
export function WidgetStocks() {
  const { tickers, error, add, remove } = useStocks();
  return (
    <StocksBoard
      tickers={tickers}
      error={error}
      onAdd={add}
      onRemove={(s) => void remove(s)}
    />
  );
}

export function StocksBoard({
  tickers,
  error,
  onAdd,
  onRemove,
}: {
  /** `null` until the first quotes land. */
  tickers: Ticker[] | null;
  error: string | null;
  onAdd: (symbol: string) => Promise<unknown>;
  onRemove: (symbol: string) => void;
}) {
  if (!tickers) {
    return (
      <WidgetBoard label="Stocks">
        <Notice
          size="large"
          label="Stocks"
          title={error ? "Could not load quotes" : "Fetching quotes"}
        >
          {error ?? "Prices come from the symbols you follow."}
        </Notice>
      </WidgetBoard>
    );
  }
  return (
    <WidgetBoard label="Stocks">
      {tickers.length > 0 ? (
        <ListTile tickers={tickers} onRemove={onRemove} />
      ) : (
        <Notice size="large" label="Stocks" title="Nothing followed yet">
          Add a symbol to see its price here: a stock, an index or a pair like
          BTC-USD.
        </Notice>
      )}
      <FollowTile error={error} onAdd={onAdd} />
    </WidgetBoard>
  );
}

/** A change as a pill: the sign is in the words, the fill only agrees with it. */
function Change({ pct }: { pct: number | null }) {
  const move = moveOf(pct);
  if (pct === null || move === null) return null;
  return (
    <span
      data-w-role={
        move === "up" ? "ok" : move === "down" ? "calendar" : undefined
      }
      className={cn(
        "rounded-notch-chip px-1.5 py-0.5 text-notch-w-meta leading-none font-semibold tabular-nums",
        move === "flat" ? "bg-widget-neutral-track" : "bg-widget-fill",
      )}
    >
      {percent(pct)}
    </span>
  );
}

const CHART_W = 76;
const CHART_H = 34;

/**
 * The day's line against yesterday's close. A thin 1.75px stroke in the
 * up/down colour with the last bar ringed in the tile's own colour, a dashed
 * baseline at the previous close, and no axes: at this size the price and the
 * pill beside it carry the numbers. A day with no series says so in words
 * rather than drawing nothing.
 */
function DayChart({ t }: { t: Ticker }) {
  const chart = dayChart(t.series, t.previousClose, CHART_W, CHART_H);
  if (!chart) {
    return (
      <span
        style={{ width: CHART_W }}
        className="shrink-0 text-center text-notch-w-over leading-tight font-medium text-widget-ink-2"
      >
        No chart yet
      </span>
    );
  }
  const role = chart.move === "up" ? "ok" : chart.move === "down" ? "calendar" : undefined;
  const word = { up: "up", down: "down", flat: "flat" }[chart.move];
  return (
    <svg
      role="img"
      aria-label={`${t.symbol} today, ${word} on the previous close`}
      data-w-role={role}
      viewBox={`0 0 ${CHART_W} ${CHART_H}`}
      width={CHART_W}
      height={CHART_H}
      className="shrink-0 overflow-visible"
    >
      {chart.baseline !== null && (
        <line
          x1={0}
          x2={CHART_W}
          y1={chart.baseline}
          y2={chart.baseline}
          strokeWidth={1}
          strokeDasharray="2 3"
          className="stroke-widget-neutral-track"
        />
      )}
      <path
        d={chart.line}
        fill="none"
        strokeWidth={1.75}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={role ? "stroke-widget-acc" : "stroke-widget-ink-2"}
      />
      <circle cx={chart.end[0]} cy={chart.end[1]} r={3.6} className="fill-widget-tile" />
      <circle cx={chart.end[0]} cy={chart.end[1]} r={2.2} className={role ? "fill-widget-acc" : "fill-widget-ink-2"} />
    </svg>
  );
}

/** Stocks, large: every symbol followed, scrolling when there are more than fit. */
export function ListTile({
  tickers,
  onRemove,
}: {
  tickers: Ticker[];
  onRemove: (symbol: string) => void;
}) {
  return (
    <WidgetFrame size="large" label="Stocks">
      <ul
        aria-label="Stocks"
        tabIndex={0}
        className="m-0 h-full list-none overflow-y-auto p-0 px-3.5 outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-solid focus-visible:outline-notch-focus"
      >
        {tickers.map((t) => (
          <li
            key={t.symbol}
            className="flex items-center gap-2 border-b border-widget-sep py-2.5 last:border-b-0"
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate text-notch-w-body leading-tight font-semibold">
                {t.symbol}
              </span>
              <span className="block truncate text-notch-w-meta leading-tight font-medium text-widget-ink-2">
                {t.error ??
                  [t.name, marketLabel(t)].filter(Boolean).join(" · ")}
              </span>
            </span>
            {t.price != null && <DayChart t={t} />}
            {t.price != null && (
              <span className="flex w-[78px] shrink-0 flex-col items-end gap-1">
                <span className="text-notch-w-body leading-none font-semibold tabular-nums">
                  {price(t.price)}
                </span>
                <Change pct={t.changePct} />
              </span>
            )}
            <button
              type="button"
              aria-label={`Stop following ${t.symbol}`}
              onClick={() => onRemove(t.symbol)}
              className={cn(
                NOTCH_FOCUS,
                "grid size-6 shrink-0 place-items-center rounded-full text-widget-ink-3",
                "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:bg-widget-tile-2 hover:text-widget-ink",
              )}
            >
              <XIcon aria-hidden className="size-3.5" strokeWidth={2.2} />
            </button>
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

/** Stocks, follow: the one field, and the reason when a symbol is refused. */
export function FollowTile({
  error,
  onAdd,
}: {
  error: string | null;
  onAdd: (symbol: string) => Promise<unknown>;
}) {
  const [symbol, setSymbol] = useState("");
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!symbol.trim()) return;
    await onAdd(symbol);
    setSymbol("");
  };
  return (
    <WidgetFrame size="medium" label="Follow">
      <form
        onSubmit={(e) => void submit(e)}
        className="flex h-full flex-col justify-between p-3.5"
      >
        <div>
          <div className="text-notch-w-headline leading-tight font-semibold">
            Follow a symbol
          </div>
          <div className="text-notch-w-meta font-medium text-widget-ink-2">
            Quotes refresh every minute while this is open.
          </div>
        </div>
        <div>
          {error && (
            <p
              role="status"
              className="m-0 mb-1 truncate text-notch-w-meta text-notch-danger"
            >
              {error}
            </p>
          )}
          <div className="flex items-center gap-1.5">
            <TextField
              aria-label="Ticker symbol"
              placeholder="AAPL, ^GSPC, BTC-USD"
              value={symbol}
              onChange={(e) => setSymbol(e.target.value)}
              className="min-w-0 flex-1"
            />
            <Pill type="submit">Add</Pill>
          </div>
        </div>
      </form>
    </WidgetFrame>
  );
}
