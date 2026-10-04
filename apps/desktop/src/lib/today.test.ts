import { describe, expect, it } from "vitest";
import type { Event as CalendarEvent } from "@/lib/bindings";
import { degrees, nextEvent, dueText, marketLabel, overdue, percent, untilText, usesFahrenheit } from "./today";

describe("usesFahrenheit", () => {
  it("goes by region, not language", () => {
    expect(usesFahrenheit("en-US")).toBe(true);
    expect(usesFahrenheit("en-GB")).toBe(false);
    expect(usesFahrenheit("en-CA")).toBe(false);
    expect(usesFahrenheit("fr-FR")).toBe(false);
    // No region to go by, or not a locale at all: Celsius, never a throw.
    expect(usesFahrenheit("en")).toBe(false);
    expect(usesFahrenheit("not a locale")).toBe(false);
  });
});

describe("degrees", () => {
  it("rounds and never shows negative zero", () => {
    expect(degrees(20.5)).toBe("21°");
    expect(degrees(-0.4)).toBe("0°");
    expect(degrees(-3.2)).toBe("-3°");
    expect(degrees(null)).toBe("–");
  });
});

describe("marketLabel", () => {
  // A stale price must never read as live: delayed outranks the market state.
  it("says when a price is not now", () => {
    expect(marketLabel({ state: "open", delayed: false })).toBe("");
    expect(marketLabel({ state: "open", delayed: true })).toBe("delayed");
    expect(marketLabel({ state: "after", delayed: false })).toBe("after hours");
    expect(marketLabel({ state: "closed", delayed: false })).toBe("closed");
    expect(marketLabel({ state: null, delayed: false })).toBe("");
  });
});

describe("percent", () => {
  it("signs a move with a word-shaped sign, and calls a flat day flat", () => {
    expect(percent(0.84)).toBe("+0.8%");
    expect(percent(-1.24)).toBe("−1.2%");
    expect(percent(0.02)).toBe("0.0%");
    expect(percent(-0.04)).toBe("0.0%");
  });
});

describe("reminders", () => {
  const at = (iso: string) => new Date(iso).getTime();
  const now = at("2026-10-04T09:00:00");

  // The failure: an all-day reminder stored as today 00:00 reads as overdue
  // from the first minute of its own day.
  it("is overdue by its time when timed and by its day when all-day", () => {
    expect(overdue(at("2026-10-04T00:00:00"), true, now)).toBe(false);
    expect(overdue(at("2026-10-03T00:00:00"), true, now)).toBe(true);
    expect(overdue(at("2026-10-04T08:59:00"), false, now)).toBe(true);
    expect(overdue(at("2026-10-04T16:00:00"), false, now)).toBe(false);
  });

  it("says when in words", () => {
    expect(dueText(at("2026-10-04T00:00:00"), true, now, "en-US")).toBe("Today");
    expect(dueText(at("2026-10-04T16:00:00"), false, now, "en-US")).toBe("4:00 PM");
    expect(dueText(at("2026-10-03T00:00:00"), true, now, "en-US")).toBe("Overdue · Sat");
    expect(dueText(at("2026-09-28T12:00:00"), false, now, "en-US")).toBe("Overdue · Mon 12:00 PM");
    expect(dueText(at("2026-10-04T07:00:00"), false, now, "en-US")).toBe("Overdue · 7:00 AM");
  });
});

describe("untilText", () => {
  const min = 60_000;
  it("counts up to the minute and calls a running event now", () => {
    expect(untilText(25 * min, 55 * min, 0)).toBe("in 25 min");
    expect(untilText(1, 55 * min, 0)).toBe("in 1 min");
    expect(untilText(90 * min, 120 * min, 0)).toBe("in 1 hr 30 min");
    expect(untilText(120 * min, 150 * min, 0)).toBe("in 2 hr");
    expect(untilText(-5 * min, 20 * min, 0)).toBe("now");
  });
});

describe("nextEvent", () => {
  const min = 60_000;
  const ev = (id: string, start: number, end: number, allDay = false): CalendarEvent => ({
    id, title: id, start: start * min, end: end * min, allDay, location: null, joinUrl: null, calendar: "Work",
  });
  // The view and the island must agree on which event is next.
  it("prefers what starts within the hour to what is running, and skips all-day", () => {
    const running = ev("running", -30, 60);
    const soon = ev("soon", 25, 55);
    expect(nextEvent([running, soon, ev("later", 90, 120)], 0)?.id).toBe("soon");
    expect(nextEvent([running, ev("later", 90, 120)], 0)?.id).toBe("running");
    expect(nextEvent([ev("holiday", -60, 600, true)], 0)).toBeUndefined();
    expect(nextEvent([ev("over", -90, -30)], 0)).toBeUndefined();
  });
});
