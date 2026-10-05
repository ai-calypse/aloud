import { useState } from "react";
import { CheckIcon, ListIcon } from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { Numeral, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { commands, type AgendaError, type Reminder } from "@/lib/bindings";
import { dueText, overdue } from "@/lib/today";
import { cn } from "@/lib/utils";
import { useNow, useReminders } from "../../useToday";
import { Aside, AgendaNotice, Notice, QuietPill } from "./shared";
import { reminderCounts } from "./today-logic";

/**
 * Today › Reminders in the widgets look: Apple's Reminders widget (a red count
 * and title over rows on hairlines, an unfilled radio for each) with two small
 * tiles that say how many are due and how many have slipped. Ticking one off
 * is the same single write as in the calm view, to Reminders itself.
 */
export function WidgetReminders() {
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
    if (res.status === "ok")
      setItems((items ?? []).filter((x) => x.id !== r.id));
    else
      setFailed(
        res.error.kind === "failed"
          ? res.error.message
          : "Aloud is not allowed to change Reminders.",
      );
  };

  return (
    <RemindersBoard
      items={items}
      error={error}
      now={now}
      ticking={ticking}
      failed={failed}
      onTick={(r) => void tick(r)}
      onReload={() => void reload()}
    />
  );
}

export function RemindersBoard({
  items,
  error,
  now,
  ticking,
  failed,
  onTick,
  onReload,
}: {
  items: Reminder[] | null;
  error: AgendaError | null;
  now: number;
  /** Ids whose tick is in flight: drawn done, not yet gone. */
  ticking: ReadonlySet<string>;
  failed: string | null;
  onTick: (r: Reminder) => void;
  onReload: () => void;
}) {
  if (!items) {
    return (
      <WidgetBoard label="Reminders">
        {error ? (
          <AgendaNotice
            error={error}
            app="Reminders"
            permission="reminders"
            retry={onReload}
            size="large"
          />
        ) : (
          <Notice size="large" label="Reminders" title="Reading your reminders">
            The first read opens Reminders.
          </Notice>
        )}
      </WidgetBoard>
    );
  }
  if (items.length === 0) {
    return (
      <WidgetBoard label="Reminders">
        <Notice
          size="large"
          label="Reminders"
          title="Nothing due"
          action={<QuietPill onClick={onReload}>Refresh</QuietPill>}
        >
          Reminders due today, and any that slipped past, show up here.
        </Notice>
      </WidgetBoard>
    );
  }
  const counts = reminderCounts(items, now);
  return (
    <>
      <WidgetBoard label="Reminders">
        <ListTile items={items} now={now} ticking={ticking} onTick={onTick} />
        <CountTile label="Due today" value={counts.due} word="due" />
        <CountTile
          label="Overdue"
          value={counts.late}
          word={counts.late > 0 ? "overdue" : "none overdue"}
          late={counts.late > 0}
        />
      </WidgetBoard>
      {failed && <Aside>{failed}</Aside>}
    </>
  );
}

/** Reminders, large: the red count and title, then every reminder due or overdue, soonest first. */
export function ListTile({
  items,
  now,
  ticking,
  onTick,
}: {
  items: Reminder[];
  now: number;
  ticking: ReadonlySet<string>;
  onTick: (r: Reminder) => void;
}) {
  return (
    <WidgetFrame size="large" label="Reminders" accent="calendar">
      <div className="flex h-full flex-col p-3.5 pb-0">
        <div className="flex items-start justify-between">
          <div>
            <Numeral text={String(items.length)} size="m" />
            <div className="text-notch-w-headline leading-tight font-semibold text-widget-calendar">
              Reminders
            </div>
          </div>
          <span
            aria-hidden
            className="grid size-8 place-items-center rounded-full bg-widget-calendar text-widget-ink"
          >
            <ListIcon className="size-4.5" strokeWidth={2.4} />
          </span>
        </div>
        {/* Scrolls when more are due than fit; a stop of its own for the keyboard. */}
        <ul
          aria-label="Reminders due"
          tabIndex={0}
          className="m-0 mt-2 min-h-0 flex-1 list-none overflow-y-auto p-0 pb-3.5 outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-solid focus-visible:outline-notch-focus"
        >
          {items.map((r) => {
            const done = ticking.has(r.id);
            const late = overdue(r.due, r.allDay, now);
            return (
              <li
                key={r.id}
                className="flex items-center gap-2.5 border-b border-widget-sep py-1.5 last:border-b-0"
              >
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={done}
                  aria-label={`Mark done: ${r.title}`}
                  disabled={done}
                  onClick={() => onTick(r)}
                  className={cn(
                    NOTCH_FOCUS,
                    "grid size-5 shrink-0 place-items-center rounded-full border-2 border-widget-radio",
                    "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)] hover:border-widget-ink-2",
                    done &&
                      "border-widget-calendar bg-widget-calendar text-widget-ink",
                  )}
                >
                  {done && (
                    <CheckIcon
                      aria-hidden
                      className="size-3"
                      strokeWidth={3.5}
                    />
                  )}
                </button>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-notch-w-body leading-tight">
                    {r.title}
                  </span>
                  <span
                    className={cn(
                      "block truncate text-notch-w-meta leading-tight font-medium",
                      // "Overdue" is in the words, so the colour only agrees with it.
                      late ? "text-widget-calendar" : "text-widget-ink-2",
                    )}
                  >
                    {[dueText(r.due, r.allDay, now), r.list]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                </span>
              </li>
            );
          })}
        </ul>
      </div>
    </WidgetFrame>
  );
}

/** Reminders, small: one number and what it counts. Overdue turns amber, with its word. */
export function CountTile({
  label,
  value,
  word,
  late = false,
}: {
  label: string;
  value: number;
  word: string;
  late?: boolean;
}) {
  return (
    <WidgetFrame
      size="small"
      label={label}
      accent={late ? "orange" : "calendar"}
    >
      <div className="flex h-full flex-col justify-between p-3.5">
        <span
          aria-hidden
          className={cn(
            "grid size-8 place-items-center rounded-full text-widget-ink",
            late ? "bg-widget-fill" : "bg-widget-calendar",
          )}
        >
          <ListIcon className="size-4.5" strokeWidth={2.4} />
        </span>
        <div>
          <Numeral text={String(value)} size="l" />
          <div
            className={cn(
              "text-notch-w-headline leading-tight font-semibold",
              late ? "text-widget-acc-ink" : "text-widget-calendar",
            )}
          >
            {word}
          </div>
        </div>
      </div>
    </WidgetFrame>
  );
}
