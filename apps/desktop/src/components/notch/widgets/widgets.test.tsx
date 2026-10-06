import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Capsule, EventCard, Numeral, Ring, RingStack, SegBar, Spark, WidgetBoard, WidgetFrame } from ".";

const html = (node: React.ReactElement) => renderToStaticMarkup(node);

describe("Ring", () => {
  it("is a meter that says its value in words, not only in colour", () => {
    const out = html(<Ring value={0.76} label="Focus" valueText="19 of 25 minutes" accent="move" />);
    expect(out).toContain('role="meter"');
    expect(out).toContain('aria-label="Focus"');
    expect(out).toContain('aria-valuenow="76"');
    expect(out).toContain('aria-valuetext="19 of 25 minutes"');
    expect(out).toContain('data-w-role="move"');
  });

  it("starts a spring ring empty, so the effect fills it rather than flashing it full", () => {
    // size 52, stroke 9: r 21.5, circumference ~135.1
    const out = html(<Ring value={1} label="x" />);
    expect(out).toMatch(/stroke-dashoffset="135\.\d+"/);
  });

  it("draws a drain ring at its value from the first frame", () => {
    const out = html(<Ring value={0.5} label="x" motion="drain" />);
    expect(out).toContain('data-motion="drain"');
    expect(out).toMatch(/stroke-dashoffset:67\.\d+/);
  });

  it("hides the arc at zero, where a round cap would still draw a dot", () => {
    expect(html(<Ring value={0} label="x" motion="none" />)).toContain('opacity="0"');
  });
});

describe("RingStack", () => {
  it("is one group holding a meter per ring", () => {
    const out = html(
      <RingStack
        label="Activity"
        rings={[
          { value: 0.4, label: "Move", accent: "move" },
          { value: 0.7, label: "Exercise", accent: "exercise" },
          { value: 0.2, label: "Stand", accent: "stand" },
        ]}
      />,
    );
    expect(out).toContain('role="group"');
    expect(out.match(/role="meter"/g)).toHaveLength(3);
  });
});

describe("Capsule", () => {
  it("is a read-only meter without onChange", () => {
    const out = html(<Capsule value={0.42} label="Battery" />);
    expect(out).toContain('role="meter"');
    expect(out).not.toContain("<input");
  });

  it("is a native range input with onChange, so keys and drag are the platform's", () => {
    const out = html(<Capsule value={0.42} label="Position" onChange={() => {}} />);
    expect(out).toContain('type="range"');
    expect(out).toContain('aria-label="Position"');
    expect(out).toContain('value="42"');
  });
});

// The Calm Home props are all optional, and the widget boards must draw exactly
// what they did: each default below is the class it always had, with no style.
describe("Calm primitive props", () => {
  it("fills a vertical capsule from the bottom, ending flat and filling its parent", () => {
    const out = html(<Capsule value={0.25} label="Battery" vertical />);
    expect(out).toContain("inset(75% 0 0 0)");
    expect(out).toContain("h-full");
    expect(out).not.toContain("h-1.5");
    expect(out).not.toContain("999px");
  });

  it("leaves a horizontal capsule as it was", () => {
    const out = html(<Capsule value={0.25} label="Battery" />);
    expect(out).toContain("h-1.5");
    expect(out).toContain("inset(0 75% 0 0 round 999px)");
  });

  it("sizes a segmented bar when told to, and keeps the 6px bar when not", () => {
    const sized = html(<SegBar value={0.5} segments={10} label="Usage" height={7} gap={3} />);
    expect(sized).toContain("height:7px");
    expect(sized).toContain("gap:3px");
    expect(sized).not.toContain("h-1.5");
    const plain = html(<SegBar value={0.5} segments={10} label="Usage" />);
    expect(plain).toContain("h-1.5");
    expect(plain).toContain("gap-0.5");
    expect(plain).not.toContain("style=");
  });

  it("tracks a Calm numeral tighter, and a widget one as before", () => {
    expect(html(<Numeral text="82" tracking="calm" />)).toContain("tracking-notch-n");
    const widget = html(<Numeral text="82" />);
    expect(widget).toContain("tracking-notch-w-numeral");
    expect(widget).not.toContain("tracking-notch-n ");
  });

  it("shades under a filled spark and ends it on a dot, and draws neither by default", () => {
    const filled = html(<Spark values={[1, 3, 2, 4]} label="Highs" fill />);
    expect(filled).toContain("<circle");
    expect(filled).toContain("Z");
    expect(html(<Spark values={[1, 3, 2, 4]} label="Highs" />)).not.toContain("<circle");
  });
});

describe("SegBar", () => {
  it("lights the nearest whole number of segments", () => {
    const out = html(<SegBar value={0.5} segments={4} label="Level" />);
    expect(out.match(/bg-widget-acc"/g)).toHaveLength(2);
    expect(out).toContain('aria-valuenow="50"');
  });
});

describe("Numeral", () => {
  it("is plain text unless it rolls", () => {
    expect(html(<Numeral text="82" />)).toContain(">82<");
  });

  it("speaks a rolling numeral once, whole, and hides the digit columns", () => {
    const out = html(<Numeral text="59" roll />);
    expect(out).toContain('<span class="sr-only">59</span>');
    expect(out).toContain('aria-hidden="true"');
    // The 5 column sits five ems up; the 9 column nine.
    expect(out).toContain("translateY(-5em)");
    expect(out).toContain("translateY(-9em)");
  });
});

describe("EventCard", () => {
  it("carries the title and time as words, with the tone only on the card", () => {
    const out = html(<EventCard title="Design sync" time="2:30–3:00 PM" tone="green" />);
    expect(out).toContain("Design sync");
    expect(out).toContain("2:30–3:00 PM");
    expect(out).toContain('data-w-event="green"');
  });
});

describe("Spark", () => {
  it("is an image with a label, and draws nothing for too few points", () => {
    expect(html(<Spark values={[1]} label="CPU" />)).not.toContain("<path");
    const out = html(<Spark values={[1, 3, 2]} label="CPU, rising" />);
    expect(out).toContain('role="img"');
    expect(out).toContain("<path");
  });
});

describe("WidgetFrame", () => {
  it("names the tile and shows its label under it", () => {
    const out = html(
      <WidgetFrame size="small" label="Battery">
        x
      </WidgetFrame>,
    );
    expect(out).toContain('aria-label="Battery"');
    expect(out).toContain('data-size="small"');
    expect(out).not.toContain("<button");
  });

  it("makes the label the one button when the tile opens something", () => {
    const out = html(
      <WidgetFrame size="medium" label="Calendar" onActivate={() => {}}>
        x
      </WidgetFrame>,
    );
    expect(out.match(/<button/g)).toHaveLength(1);
    expect(out).toContain("col-span-2");
  });

  it("applies the live treatment only with an accent to be live in", () => {
    const withAccent = html(
      <WidgetFrame size="small" label="Focus" accent="move" live>
        x
      </WidgetFrame>,
    );
    expect(withAccent).toContain("data-w-live");
    const without = html(
      <WidgetFrame size="small" label="Focus" live>
        x
      </WidgetFrame>,
    );
    expect(without).not.toContain("data-w-live");
  });

  it("spans two rows for a large tile, inside a labelled board", () => {
    const out = html(
      <WidgetBoard label="Today">
        <WidgetFrame size="large" label="Calendar">
          x
        </WidgetFrame>
      </WidgetBoard>,
    );
    expect(out).toContain("row-span-2");
    expect(out).toContain('aria-label="Today"');
  });
});
