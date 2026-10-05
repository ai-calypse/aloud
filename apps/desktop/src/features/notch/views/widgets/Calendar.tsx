import {
  EventCard,
  Numeral,
  WidgetBoard,
  WidgetFrame,
} from "@/components/notch/widgets";
import {
  commands,
  type AgendaError,
  type Event as CalendarEvent,
} from "@/lib/bindings";
import { timeOfDay, untilText } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useCalendar, useNow } from "../../useToday";
import { Aside, AgendaNotice, Notice, Pill, QuietPill } from "./shared";
import {
  GRID_HOURS,
  hourLabel,
  layoutDay,
  monthGrid,
  moreLabel,
  nowLine,
  splitAgenda,
  toneFor,
  windowStart,
} from "./today-logic";

const HOUR = 3_600_000;

/**
 * Today › Calendar in the widgets look: Apple's Calendar family. A large tile
 * (today beside tomorrow on an hour axis), a month with a red dot on today,
 * and an agenda that carries what the axis cannot: every remaining event with
 * its Join button, and the refresh when there is nothing.
 *
 * Reads the same hook as the calm view, so the first read still opens
 * Calendar on purpose (the Home board never does).
 */
export function WidgetCalendar() {
  const { items, error, reload } = useCalendar();
  const now = useNow();
  return (
    <CalendarBoard
      items={items}
      error={error}
      now={now}
      onReload={() => void reload()}
      onJoin={(url) => void commands.openMeetingLink(url)}
    />
  );
}

export function CalendarBoard({
  items,
  error,
  now,
  onReload,
  onJoin,
}: {
  /** `null` until the first read lands. */
  items: CalendarEvent[] | null;
  error: AgendaError | null;
  now: number;
  onReload: () => void;
  onJoin: (url: string) => void;
}) {
  if (!items) {
    return (
      <WidgetBoard label="Calendar">
        {error ? (
          <AgendaNotice
            error={error}
            app="Calendar"
            permission="calendar"
            retry={onReload}
            size="large"
          />
        ) : (
          <Notice size="large" label="Calendar" title="Reading your calendar">
            The first read opens Calendar and can take a few seconds.
          </Notice>
        )}
        <MonthTile now={now} />
      </WidgetBoard>
    );
  }
  const { live, today, tomorrow, next } = splitAgenda(items, now);
  return (
    <>
      <WidgetBoard label="Calendar">
        <DayTile today={today} tomorrow={tomorrow} now={now} />
        <MonthTile now={now} />
        <AgendaTile
          today={today}
          tomorrow={tomorrow}
          next={next}
          empty={live.length === 0}
          now={now}
          onReload={onReload}
          onJoin={onJoin}
        />
      </WidgetBoard>
      {error && (
        <Aside>
          Could not refresh:{" "}
          {error.kind === "failed" ? error.message : "access was refused"}
        </Aside>
      )}
    </>
  );
}

/** One column header: the day's name over what goes beside it. */
function DayHeader({
  children,
  name,
  red,
}: {
  children?: React.ReactNode;
  name: string;
  red?: boolean;
}) {
  return (
    <div className="h-14 min-w-0">
      <div
        className={cn(
          "text-notch-w-over leading-tight font-semibold tracking-wide uppercase",
          red ? "text-widget-calendar" : "text-widget-ink-2",
        )}
      >
        {name}
      </div>
      {children}
    </div>
  );
}

/** An all-day event is the day, not a block on it: a calendar-coloured bar and its title. */
function AllDay({ events }: { events: CalendarEvent[] }) {
  const first = events[0];
  if (!first) return null;
  return (
    <div
      data-w-event={toneFor(first.calendar)}
      className="flex min-w-0 items-center gap-1.5"
    >
      <span
        aria-hidden
        className="h-3.5 w-0.75 shrink-0 rounded-full bg-widget-ev-bar"
      />
      <span className="truncate text-notch-w-title font-semibold">
        {first.title}
      </span>
      {events.length > 1 && (
        <span className="shrink-0 text-notch-w-meta font-medium text-widget-ink-2">
          +{events.length - 1}
        </span>
      )}
    </div>
  );
}

/** Hour lines, the events on them, and the red line for now. */
function HourGrid({
  events,
  dayStart,
  anchorHour,
  now,
  showNow,
  empty,
}: {
  events: CalendarEvent[];
  dayStart: number;
  anchorHour: number;
  now: number;
  showNow: boolean;
  empty: string;
}) {
  const timed = events.filter((e) => !e.allDay);
  const start = windowStart(timed, dayStart, anchorHour);
  const { placed, hidden } = layoutDay(events, dayStart, start);
  const line = showNow ? nowLine(now, dayStart, start) : null;
  const hiddenTones = [
    ...new Set(hidden.map((e) => toneFor(e.calendar))),
  ].slice(0, 3);
  const more = hidden.length;

  return (
    <>
      <div role="group" aria-label="Hours" className="relative h-60">
        {Array.from({ length: GRID_HOURS }, (_, i) => (
          <div
            key={i}
            aria-hidden
            className="absolute inset-x-0 flex items-start gap-1.5"
            style={{ top: `${(i / GRID_HOURS) * 100}%` }}
          >
            <span className="w-3 -translate-y-1/2 text-right text-notch-w-over leading-none text-widget-ink-2">
              {hourLabel((start + i) % 24)}
            </span>
            <span className="h-px flex-1 bg-widget-sep" />
          </div>
        ))}
        <div className="absolute inset-y-0 right-0 left-4.5">
          {placed.map((p) => (
            <div
              key={p.event.id}
              className="absolute min-h-5 pr-0.5"
              style={{
                top: `${p.top * 100}%`,
                height: `${p.height * 100}%`,
                left: `${(p.lane / p.lanes) * 100}%`,
                width: `${100 / p.lanes}%`,
              }}
            >
              <EventCard
                title={p.event.title}
                // A short block has no room for a second line.
                time={
                  p.height * GRID_HOURS >= 0.75
                    ? timeOfDay(p.event.start)
                    : undefined
                }
                tone={toneFor(p.event.calendar)}
                className="h-full overflow-hidden"
              />
            </div>
          ))}
          {line !== null && (
            <div
              aria-hidden
              className="absolute inset-x-0 z-10 flex items-center"
              style={{ top: `${line * 100}%` }}
            >
              <span className="-ml-0.5 size-1.5 shrink-0 rounded-full bg-widget-calendar" />
              <span className="h-px flex-1 bg-widget-calendar" />
            </div>
          )}
          {placed.length === 0 && (
            <div className="grid h-full place-items-center text-center text-notch-w-meta font-medium text-widget-ink-2">
              {empty}
            </div>
          )}
        </div>
      </div>
      <div className="mt-1.5 flex h-4 items-center gap-1.5 text-notch-w-meta font-medium text-widget-ink-2">
        {more > 0 && (
          <>
            <span aria-hidden className="flex gap-0.5">
              {hiddenTones.map((t) => (
                <span
                  key={t}
                  data-w-event={t}
                  className="h-3 w-0.75 rounded-full bg-widget-ev-bar"
                />
              ))}
            </span>
            <span className="truncate">{moreLabel(more)}</span>
          </>
        )}
      </div>
    </>
  );
}

/** Calendar, large: today beside tomorrow, each on its own hour axis. */
export function DayTile({
  today,
  tomorrow,
  now,
}: {
  today: CalendarEvent[];
  tomorrow: CalendarEvent[];
  now: number;
}) {
  const date = new Date(now);
  const todayStart = new Date(now).setHours(0, 0, 0, 0);
  const tomorrowStart = todayStart + 24 * HOUR;
  const firstTomorrow = tomorrow.find((e) => !e.allDay);
  return (
    <WidgetFrame size="large" label="Calendar">
      <div className="grid h-full grid-cols-2 gap-3.5 p-3.5">
        <div className="min-w-0">
          <DayHeader
            name={date.toLocaleDateString(undefined, { weekday: "long" })}
            red
          >
            <div className="flex items-end gap-2">
              <Numeral text={String(date.getDate())} size="s" />
              <div className="min-w-0 flex-1 pb-0.5">
                <AllDay events={today.filter((e) => e.allDay)} />
              </div>
            </div>
          </DayHeader>
          <HourGrid
            events={today}
            dayStart={todayStart}
            anchorHour={date.getHours()}
            now={now}
            showNow
            empty="Nothing else today"
          />
        </div>
        <div className="min-w-0">
          <DayHeader name="Tomorrow">
            <div className="mt-1.5">
              <AllDay events={tomorrow.filter((e) => e.allDay)} />
            </div>
          </DayHeader>
          <HourGrid
            events={tomorrow}
            dayStart={tomorrowStart}
            anchorHour={
              firstTomorrow ? new Date(firstTomorrow.start).getHours() : 9
            }
            now={now}
            showNow={false}
            empty="Nothing tomorrow"
          />
        </div>
      </div>
    </WidgetFrame>
  );
}

/** Calendar, month: the date as Apple sets it, and the month with a red dot on today. */
export function MonthTile({ now }: { now: number }) {
  const date = new Date(now);
  const month = monthGrid(now);
  return (
    <WidgetFrame size="medium" label="Month">
      <div className="flex h-full gap-4 p-3.5">
        <div className="flex w-24 shrink-0 flex-col justify-between">
          <div>
            <div className="text-notch-w-over font-semibold tracking-wide text-widget-calendar uppercase">
              {date.toLocaleDateString(undefined, { weekday: "long" })}
            </div>
            <Numeral text={String(date.getDate())} size="l" />
          </div>
          <div className="text-notch-w-meta leading-tight font-medium text-widget-ink-2">
            {month.title}
          </div>
        </div>
        <table
          aria-label={month.title}
          className="min-w-0 flex-1 border-collapse text-center text-notch-w-over"
        >
          <thead>
            <tr>
              {month.weekdays.map((d, i) => (
                <th
                  key={i}
                  scope="col"
                  className="h-4 font-semibold text-widget-ink-2"
                >
                  {d}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {month.weeks.map((week, w) => (
              <tr key={w}>
                {week.map((day, i) => (
                  <td key={i} className="h-4 p-0">
                    {day !== null && (
                      <span
                        aria-current={day === month.today ? "date" : undefined}
                        className={cn(
                          "mx-auto grid size-4 place-items-center rounded-full font-medium tabular-nums",
                          day === month.today
                            ? "bg-widget-calendar text-widget-ink"
                            : "text-widget-ink",
                        )}
                      >
                        {day}
                      </span>
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </WidgetFrame>
  );
}

/**
 * Calendar, agenda: everything the axis does not carry. Every remaining event
 * for today and tomorrow, so an event in "N more events" is still here with
 * its Join; the next one's button is the inverted one, as in the calm view.
 */
export function AgendaTile({
  today,
  tomorrow,
  next,
  empty,
  now,
  onReload,
  onJoin,
}: {
  today: CalendarEvent[];
  tomorrow: CalendarEvent[];
  next: CalendarEvent | undefined;
  empty: boolean;
  now: number;
  onReload: () => void;
  onJoin: (url: string) => void;
}) {
  if (empty) {
    return (
      <Notice
        label="Agenda"
        title="Nothing else on today or tomorrow"
        action={<QuietPill onClick={onReload}>Refresh</QuietPill>}
      >
        Events from the calendars you can edit show up here. Holiday and
        subscribed calendars are left out.
      </Notice>
    );
  }
  const group = (title: string, events: CalendarEvent[]) =>
    events.length > 0 && (
      <section key={title} aria-label={title}>
        <div className="mb-1 text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
          {title}
        </div>
        <ul className="m-0 grid list-none gap-1.5 p-0">
          {events.map((e) => {
            const isNext = e.id === next?.id;
            const when = e.allDay
              ? "All day"
              : `${timeOfDay(e.start)} – ${timeOfDay(e.end)}`;
            return (
              <li key={e.id} className="flex items-center gap-2">
                <EventCard
                  className="min-w-0 flex-1"
                  title={e.title}
                  tone={toneFor(e.calendar)}
                  time={[
                    when,
                    isNext ? untilText(e.start, e.end, now) : null,
                    e.location ?? e.calendar,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                />
                {e.joinUrl && (
                  <Pill
                    className={cn(
                      !isNext && "bg-widget-tile-2 text-widget-ink",
                    )}
                    aria-label={`Join ${e.title}`}
                    onClick={() => onJoin(e.joinUrl!)}
                  >
                    Join
                  </Pill>
                )}
              </li>
            );
          })}
        </ul>
      </section>
    );
  return (
    <WidgetFrame size="medium" label="Agenda">
      {/* A scroll area has to be a stop of its own for the keyboard. */}
      <div
        role="region"
        aria-label="Remaining events"
        tabIndex={0}
        className="flex h-full flex-col gap-2 overflow-y-auto p-3.5 outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-solid focus-visible:outline-notch-focus"
      >
        {group("Today", today)}
        {group("Tomorrow", tomorrow)}
      </div>
    </WidgetFrame>
  );
}
