import { describe, expect, it } from "vitest";
import type { Event as CalendarEvent, Reminder } from "@/lib/bindings";
import {
  dayChart,
  dayStartOf,
  eventsOnDay,
  dayLabel,
  GRID_HOURS,
  hourLabel,
  layoutDay,
  monthGrid,
  moreLabel,
  moveOf,
  moveSelection,
  nowLine,
  rangeBar,
  sameDay,
  shiftDay,
  shiftMonth,
  reminderCounts,
  splitAgenda,
  toneFor,
  weekRange,
  windowStart,
} from "./today-logic";

const HOUR = 3_600_000;
// 2026-10-04 is a Sunday; local midnight keeps the tests in the machine's own zone.
const DAY = new Date(2026, 9, 4).getTime();
const at = (h: number, m = 0) => DAY + h * HOUR + m * 60_000;

const event = (id: string, from: number, to: number, extra: Partial<CalendarEvent> = {}): CalendarEvent => ({
  id,
  title: id,
  start: from,
  end: to,
  allDay: false,
  location: null,
  joinUrl: null,
  calendar: "Work",
  ...extra,
});

describe("splitAgenda", () => {
  it("drops what has ended and splits today from tomorrow at local midnight", () => {
    const items = [
      event("done", at(8), at(9)),
      event("now", at(9), at(11)),
      event("late", at(23), at(24, 30)),
      event("tomorrow", at(24, 1), at(25)),
    ];
    const r = splitAgenda(items, at(10));
    expect(r.today.map((e) => e.id)).toEqual(["now", "late"]);
    expect(r.tomorrow.map((e) => e.id)).toEqual(["tomorrow"]);
    expect(r.next?.id).toBe("now");
  });
});

describe("toneFor", () => {
  it("gives a calendar the same colour every time, from the four a card knows", () => {
    expect(toneFor("Work")).toBe(toneFor("Work"));
    expect(["green", "yellow", "blue", "red"]).toContain(toneFor("Home"));
  });
});

describe("moreLabel", () => {
  it("agrees with the count", () => {
    expect(moreLabel(1)).toBe("1 more event");
    expect(moreLabel(3)).toBe("3 more events");
  });
});

describe("layoutDay", () => {
  it("places an event by its start and length down the window", () => {
    const { placed } = layoutDay([event("a", at(10), at(11))], DAY, 9);
    expect(placed[0].top).toBeCloseTo(1 / GRID_HOURS);
    expect(placed[0].height).toBeCloseTo(1 / GRID_HOURS);
  });

  // An event running across the window's top edge is still on screen.
  it("clips an event that starts before the window and keeps it", () => {
    const { placed, hidden } = layoutDay([event("a", at(8), at(10))], DAY, 9);
    expect(hidden).toEqual([]);
    expect(placed[0].top).toBe(0);
    expect(placed[0].height).toBeCloseTo(1 / GRID_HOURS);
  });

  it("hands back what is wholly outside the window, and skips all-day events", () => {
    const events = [
      event("early", at(6), at(7)),
      event("in", at(10), at(11)),
      event("after", at(17), at(18)),
      event("day", DAY, DAY + 24 * HOUR, { allDay: true }),
    ];
    const { placed, hidden } = layoutDay(events, DAY, 9);
    expect(placed.map((p) => p.event.id)).toEqual(["in"]);
    expect(hidden.map((e) => e.id)).toEqual(["early", "after"]);
  });

  it("puts overlapping events side by side and leaves a later one alone", () => {
    const { placed } = layoutDay(
      [event("a", at(9), at(11)), event("b", at(10), at(12)), event("c", at(13), at(14))],
      DAY,
      9,
    );
    const by = Object.fromEntries(placed.map((p) => [p.event.id, p]));
    expect([by.a.lane, by.b.lane]).toEqual([0, 1]);
    expect([by.a.lanes, by.b.lanes]).toEqual([2, 2]);
    expect([by.c.lane, by.c.lanes]).toEqual([0, 1]);
  });

  it("reuses a freed lane within one cluster", () => {
    const { placed } = layoutDay(
      [event("a", at(9), at(10)), event("b", at(9), at(12)), event("c", at(10, 30), at(11))],
      DAY,
      9,
    );
    expect(placed.find((p) => p.event.id === "c")?.lane).toBe(0);
  });
});

describe("windowStart", () => {
  it("starts at the anchor when something is in view", () => {
    expect(windowStart([event("a", at(14), at(15))], DAY, 13)).toBe(13);
  });

  it("jumps to the first event past the edge rather than opening on empty hours", () => {
    expect(windowStart([event("a", at(20), at(21))], DAY, 9)).toBe(17);
  });

  it("never runs the axis off the end of the day", () => {
    expect(windowStart([], DAY, 22)).toBe(24 - GRID_HOURS);
  });
});

describe("nowLine", () => {
  it("is a fraction inside the window and absent outside it", () => {
    expect(nowLine(at(12), DAY, 9)).toBeCloseTo(3 / GRID_HOURS);
    expect(nowLine(at(8), DAY, 9)).toBeNull();
    expect(nowLine(at(17), DAY, 9)).toBeNull();
  });
});

describe("hourLabel", () => {
  it("drops the meridiem on a 12-hour axis and keeps a 24-hour one whole", () => {
    expect(hourLabel(9, "en-US")).toBe("9");
    expect(hourLabel(13, "en-US")).toBe("1");
    expect(hourLabel(13, "de-DE")).toMatch(/13/);
  });
});

describe("monthGrid", () => {
  it("starts the first on the right weekday for a Sunday or a Monday week", () => {
    // 1 October 2026 is a Thursday.
    const sun = monthGrid(DAY, 0, "en-US");
    expect(sun.weeks[0]).toEqual([null, null, null, null, 1, 2, 3]);
    expect(sun.weekdays).toEqual(["S", "M", "T", "W", "T", "F", "S"]);
    const mon = monthGrid(DAY, 1, "en-US");
    expect(mon.weeks[0]).toEqual([null, null, null, 1, 2, 3, 4]);
    expect(mon.weekdays[0]).toBe("M");
  });

  it("holds every day once, in full weeks, and marks today", () => {
    const m = monthGrid(DAY, 0, "en-US");
    const days = m.weeks.flat().filter((d): d is number => d !== null);
    expect(days).toEqual(Array.from({ length: 31 }, (_, i) => i + 1));
    expect(m.weeks.every((w) => w.length === 7)).toBe(true);
    expect(m.today).toBe(4);
    expect(m.title).toBe("October 2026");
  });

  // February of a non-leap year starting on the week's first day is exactly four weeks.
  it("does not add a blank week", () => {
    const feb = monthGrid(new Date(2026, 1, 10).getTime(), 0, "en-US");
    expect(feb.weeks).toHaveLength(4);
  });
});

describe("reminderCounts", () => {
  const r = (due: number, allDay = false): Reminder => ({ id: String(due), title: "t", due, allDay, list: "L" });
  it("separates what has slipped from what is due", () => {
    const now = at(12);
    expect(reminderCounts([r(at(9)), r(at(15)), r(at(0), true), r(DAY - HOUR, true)], now)).toEqual({
      total: 4,
      late: 2,
      due: 2,
    });
  });
});

describe("dayLabel", () => {
  it("calls the first day Today and names the rest from the place's own date", () => {
    expect(dayLabel("2026-10-04", true, "en-US")).toBe("Today");
    expect(dayLabel("2026-10-05", false, "en-US")).toBe("Mon");
  });
});

describe("rangeBar", () => {
  it("sits a day on the week's own range", () => {
    expect(rangeBar(10, 20, 0, 40)).toEqual({ left: 0.25, width: 0.25 });
  });

  it("draws nothing for a missing reading and a full bar for a flat week", () => {
    expect(rangeBar(null, 20, 0, 40)).toBeNull();
    expect(rangeBar(10, 10, 10, 10)).toEqual({ left: 0, width: 1 });
  });

  it("keeps a one-degree day visible and on the track", () => {
    const b = rangeBar(40, 40, 0, 40)!;
    expect(b.width).toBeGreaterThan(0);
    expect(b.left + b.width).toBeLessThanOrEqual(1);
  });
});

describe("weekRange", () => {
  it("takes the coldest low and the hottest high, skipping blanks", () => {
    expect(
      weekRange([
        { low: 5, high: 12 },
        { low: null, high: null },
        { low: 2, high: 9 },
      ]),
    ).toEqual({ low: 2, high: 12 });
    expect(weekRange([{ low: null, high: null }])).toBeNull();
  });
});

describe("moveOf", () => {
  it("keeps the calm list's dead zone", () => {
    expect(moveOf(0.85)).toBe("up");
    expect(moveOf(-1.2)).toBe("down");
    expect(moveOf(0.03)).toBe("flat");
    expect(moveOf(null)).toBeNull();
  });
});

describe("dayChart", () => {
  const W = 100;
  const H = 40;
  const pts = (...ys: number[]) => ys.map((y, i) => ({ x: i / (ys.length - 1), y }));

  it("draws the baseline on the chart even when the day never reaches it", () => {
    // Up the whole day, yesterday's close well below: the box has to stretch to hold it.
    const c = dayChart(pts(110, 112, 115), 100, W, H)!;
    expect(c.baseline).toBe(H - 3);
    expect(c.end[1]).toBe(3);
    expect(c.move).toBe("up");
  });

  it("stops short of the right edge on a day still in progress", () => {
    const half = [{ x: 0, y: 1 }, { x: 0.5, y: 2 }];
    expect(dayChart(half, 1, W, H)!.end[0]).toBe(W / 2);
  });

  it("colours the day by where it ended against yesterday's close, not against where it opened", () => {
    // Opened above the close, faded, ended below it: down, whatever the climb.
    expect(dayChart(pts(105, 99), 100, W, H)!.move).toBe("down");
    // Without a close it can only be read against the open.
    expect(dayChart(pts(100, 101), null, W, H)!.move).toBe("up");
    expect(dayChart(pts(100, 100.01), 100, W, H)!.move).toBe("flat");
  });

  it("drops a NaN bar instead of drawing it as zero, and says nothing for less than a line", () => {
    const withGap = [{ x: 0, y: 5 }, { x: 0.3, y: null }, { x: 1, y: 6 }];
    expect(dayChart(withGap, null, W, H)!.line.match(/[ML]/g)).toHaveLength(2);
    expect(dayChart([{ x: 0, y: 5 }], 5, W, H)).toBeNull();
    expect(dayChart([], 5, W, H)).toBeNull();
  });

  it("holds a flat day mid-box rather than dividing by zero", () => {
    const c = dayChart(pts(7, 7, 7), 7, W, H)!;
    expect(c.end[1]).toBe(H / 2);
    expect(c.baseline).toBe(H / 2);
  });
});

describe("picking a date", () => {
  it("steps by calendar days across a clock change", () => {
    // US clocks went back on 2026-11-01, so that Sunday is 25 hours long.
    const sat = new Date(2026, 10, 1 - 1).getTime();
    expect(new Date(shiftDay(sat, 1)).getDate()).toBe(1);
    expect(new Date(shiftDay(sat, 2)).getHours()).toBe(0);
    expect(new Date(shiftDay(new Date(2026, 9, 31).getTime(), 1)).getMonth()).toBe(10);
  });

  it("moves a day with left and right, a week with up and down, and ignores the rest", () => {
    expect(moveSelection("ArrowRight", DAY)).toBe(new Date(2026, 9, 5).getTime());
    expect(moveSelection("ArrowLeft", DAY)).toBe(new Date(2026, 9, 3).getTime());
    expect(moveSelection("ArrowDown", DAY)).toBe(new Date(2026, 9, 11).getTime());
    expect(moveSelection("ArrowUp", DAY)).toBe(new Date(2026, 9, 4 - 7).getTime());
    expect(moveSelection("Enter", DAY)).toBeNull();
  });

  it("goes by whole months and lands on the first", () => {
    expect(shiftMonth(DAY, 1)).toBe(new Date(2026, 10, 1).getTime());
    expect(shiftMonth(new Date(2026, 0, 31).getTime(), 1)).toBe(new Date(2026, 1, 1).getTime());
    expect(shiftMonth(DAY, -10)).toBe(new Date(2025, 11, 1).getTime());
  });

  it("knows the same day from the next, whatever the hour", () => {
    expect(sameDay(at(0, 1), at(23, 59))).toBe(true);
    expect(sameDay(at(23, 59), at(24))).toBe(false);
    expect(dayStartOf(at(15))).toBe(DAY);
  });

  // The reported gap: tapping tomorrow must not need a read of Calendar.
  it("takes a day out of a longer read, all-day first, and keeps what runs across midnight", () => {
    const tomorrow = DAY + 24 * HOUR;
    const items = [
      event("today late", at(22), at(25)),
      event("lunch", tomorrow + 12 * HOUR, tomorrow + 13 * HOUR),
      event("holiday", tomorrow, tomorrow + 24 * HOUR, { allDay: true }),
      event("earlier", at(9), at(10)),
    ];
    expect(eventsOnDay(items, tomorrow).map((e) => e.id)).toEqual(["holiday", "today late", "lunch"]);
    expect(eventsOnDay(items, DAY).map((e) => e.id)).toEqual(["earlier", "today late"]);
  });
});
