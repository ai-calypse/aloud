import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Battery, Island } from "@/lib/bindings";
import type { TilePlacement } from "@/components/notch/tile";
import { Tile } from "@/components/notch/tile";
import {
  AgentsTile,
  BatteryTile,
  CalendarTile,
  DictateTile,
  NoteTile,
  WeatherTile,
} from "./CalmTiles";

const html = (node: React.ReactElement) => renderToStaticMarkup(node);
const at = (id: string, size: TilePlacement["size"], w = 1, h = 1, col = 1, row = 1): TilePlacement => ({
  id,
  col,
  row,
  w,
  h,
  size,
});
const battery = (percent: number, state: Battery["state"]): Battery => ({
  percent,
  state,
  minutes: null,
  cycles: null,
  health: null,
});

describe("Tile", () => {
  it("sits in its cell and names itself for the tile finder", () => {
    const out = html(
      <Tile placement={at("battery", "L", 1, 2, 8, 1)} label="Battery">
        x
      </Tile>,
    );
    expect(out).toContain("grid-column:8 / span 1");
    expect(out).toContain("grid-row:1 / span 2");
    expect(out).toContain('data-tile="battery"');
    expect(out).toContain('data-size="L"');
  });

  // Controls inside a tile must not be inside the tile's own button.
  it("opens something through one button that holds no content", () => {
    const out = html(
      <Tile placement={at("x", "S")} label="Calendar" onActivate={() => {}}>
        <button type="button">Pause</button>
      </Tile>,
    );
    expect(out.match(/<button/g)).toHaveLength(2);
    expect(out).toMatch(/<button[^>]*aria-label="Calendar"[^>]*><\/button>/);
    expect(out).toContain("data-actionable");
  });

  it("has no button and does not lift when it opens nothing", () => {
    const out = html(
      <Tile placement={at("x", "S")} label="Dictate">
        x
      </Tile>,
    );
    expect(out).not.toContain("<button");
    expect(out).not.toContain("data-actionable");
  });

  it("carries its state to its parts, only when it has one", () => {
    expect(html(<Tile placement={at("x", "S")} state="caution" label="x">x</Tile>)).toContain('data-tile-state="caution"');
    expect(html(<Tile placement={at("x", "S")} label="x">x</Tile>)).not.toContain("data-tile-state");
  });
});

describe("BatteryTile", () => {
  // The colour is never the only carrier: each state says its word.
  it("says Good in green, Low in yellow and Critical in red", () => {
    for (const [pct, state, word] of [
      [82, "ok", "Good"],
      [18, "caution", "Low"],
      [8, "alert", "Critical"],
    ] as const) {
      const out = html(<BatteryTile placement={at("battery", "S")} battery={battery(pct, "discharging")} />);
      expect(out, word).toContain(`data-tile-state="${state}"`);
      expect(out, word).toContain(word);
      expect(out).toContain(`aria-valuenow="${pct}"`);
    }
  });

  it("is an honest empty tile for a Mac with no battery, and while unread", () => {
    expect(html(<BatteryTile placement={at("battery", "S")} battery={null} />)).toContain("No battery");
    expect(html(<BatteryTile placement={at("battery", "S")} battery={undefined} />)).toContain("Reading");
  });
});

describe("AgentsTile", () => {
  it("says what fills it when no agent is running", () => {
    const out = html(
      <AgentsTile placement={at("agents", "L", 2, 2)} sessions={[]} permissions={[]} voice={null} names={{}} quota={null} />,
    );
    expect(out).toContain("No agents running");
    expect(out).not.toContain("Plan usage");
  });
});

describe("CalendarTile", () => {
  const now = new Date(2026, 9, 4, 14, 5).getTime();

  // Reading the calendar can launch Calendar.app, so Home only has the island.
  it("shows the date and no invented event without the island", () => {
    const out = html(<CalendarTile placement={at("calendar", "L", 4, 2)} islands={[]} now={now} />);
    expect(out).toContain("Nothing in the next hour");
    expect(out).toContain("OCT");
  });

  it("counts down to the event the island carries, in the island's own words", () => {
    const island = { id: "calendar:next", title: "Design sync", detail: "in 25 min", kind: "live" } as Island;
    const out = html(<CalendarTile placement={at("calendar", "M", 3, 1)} islands={[island]} now={now} />);
    expect(out).toContain("Next · Design sync");
    expect(out).toContain(">25<");
    expect(out).toContain('role="meter"');
  });
});

describe("WeatherTile", () => {
  it("says it is checking, then that it is unavailable, and draws no temperature", () => {
    const tile = (loading: boolean) =>
      html(<WeatherTile placement={at("weather", "S", 2, 1)} data={null} loading={loading} />);
    expect(tile(true)).toContain("Checking the sky");
    expect(tile(false)).toContain("Weather unavailable");
  });
});

describe("quick tiles", () => {
  it("teaches Dictate's key with nothing to press, and opens New note", () => {
    const dictate = html(<DictateTile placement={at("dictate", "S")} hint="Hold Fn" />);
    expect(dictate).toContain("Hold Fn");
    expect(dictate).not.toContain("<button");
    expect(html(<NoteTile placement={at("note", "S")} onActivate={() => {}} />)).toContain("<button");
  });
});
