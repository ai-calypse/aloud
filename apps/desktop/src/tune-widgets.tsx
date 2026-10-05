import { useState, type ReactNode } from "react";
import {
  Capsule,
  EventCard,
  formatNumeral,
  Numeral,
  Ring,
  RingStack,
  SegBar,
  Spark,
  WIDGET_ROLES,
  WidgetFrame,
} from "./components/notch/widgets";
/**
 * The widget primitives' fixtures for the tune page. Fixtures are labelled as
 * such and live only here; the app never shows them.
 */

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

export function WidgetGallery() {
  const [level, setLevel] = useState(0.62);
  const [count, setCount] = useState(59);
  const [scrub, setScrub] = useState(0.35);

  return (
    <div className="grid max-w-[720px] gap-9 font-notch-w">
      <Block name="WidgetFrame" note="small, medium, large; the label under it; the live (gauge) treatment is the deepest shade of the accent">
        <div className="flex flex-wrap items-start gap-5">
          <WidgetFrame size="small" label="Plain">
            <div className="p-3.5 text-notch-w-title text-widget-ink-2">tile #1C1C1E</div>
          </WidgetFrame>
          <WidgetFrame size="small" label="Vibrant" ground="vibrant">
            <div className="p-3.5 text-notch-w-title text-widget-ink-2">tile #252525</div>
          </WidgetFrame>
          {(["move", "ok", "agent", "info", "media"] as const).map((role) => (
            <WidgetFrame key={role} size="small" label={`Live · ${role}`} accent={role} live>
              <div className="flex h-full flex-col justify-between p-3.5 text-widget-acc-ink">
                <Ring value={0.7} label={role} size={48} stroke={8} />
                <Numeral text="70" size="m" className="text-widget-acc-ink" />
              </div>
            </WidgetFrame>
          ))}
        </div>
      </Block>

      <Block name="Ring · RingStack" note="a bright arc over a track of the same hue; springs on a change, the drain ring is one tick of CSS; Fitness's three rings">
        <div className="flex flex-wrap items-center gap-8">
          <label className="grid gap-1 text-notch-meta text-notch-ink-2">
            Value {Math.round(level * 100)}%
            <input
              type="range"
              min={0}
              max={100}
              value={Math.round(level * 100)}
              onChange={(e) => setLevel(Number(e.target.value) / 100)}
            />
          </label>
          <Ring value={level} label="Spring ring" accent="ok" size={80} stroke={10}>
            <Numeral text={formatNumeral(level * 100)} size="s" />
          </Ring>
          <Ring value={level} label="Drain ring" accent="move" size={80} stroke={10} motion="drain" />
          <RingStack
            label="Activity"
            size={96}
            rings={[
              { value: level, label: "Move", accent: "move" },
              { value: Math.min(1, level + 0.2), label: "Exercise", accent: "exercise" },
              { value: Math.max(0, level - 0.3), label: "Stand", accent: "stand" },
            ]}
          />
        </div>
      </Block>

      <Block name="Capsule · SegBar" note="thin and thick; the scrubbable one is a native range input under the drawing; Tab to it, arrows move it">
        <div className="grid max-w-[360px] gap-4">
          <Capsule value={0.62} label="Battery" accent="ok" />
          <Capsule value={scrub} label="Track position" valueText={`${Math.round(scrub * 200)} of 200 seconds`} onChange={setScrub} accent="media" />
          <Capsule value={0.5} label="Volume" thick accent="info" onChange={() => {}} />
          <SegBar value={0.6} segments={10} label="Level" accent="exercise" />
        </div>
      </Block>

      <Block name="Numeral" note="Light 300, tabular; the rolling one turns each digit over on the spring timing">
        <div className="flex flex-wrap items-end gap-8">
          <Numeral text="21°" size="xl" />
          <Numeral text="82" size="l" />
          <Numeral text="4" size="s" />
          <Numeral text={String(count).padStart(2, "0")} size="l" roll />
          <button
            type="button"
            className="rounded-notch-control bg-notch-well px-2.5 py-1 text-notch-body text-notch-ink"
            onClick={() => setCount((c) => (c + 7) % 100)}
          >
            Change
          </button>
        </div>
      </Block>

      <Block name="EventCard · Spark" note="calendar tones from Figma (green, yellow) and the prototype (red, blue); a sparkline is a picture, so it has a label">
        <div className="flex flex-wrap items-center gap-5">
          <div className="grid w-44 gap-1.5">
            <EventCard title="Design sync" time="2:30–3:00 PM" tone="green" />
            <EventCard title="1:1 with Maya" time="4:00–4:30 PM" tone="yellow" />
            <EventCard title="Morning swim" time="Home" tone="red" />
            <EventCard title="Team lunch" tone="blue" />
          </div>
          <div className="grid gap-2" data-w-role="ok">
            <Spark values={[3, 5, 4, 8, 7, 9, 12, 10]} label="Fixture, rising" accent="ok" />
            <Spark values={[9, 8, 8, 5, 6, 3]} label="Fixture, falling" accent="move" />
          </div>
        </div>
      </Block>

      <Block name="Accent roles" note="each role's accent, its ink (words) and its track, on the tile">
        <div className="grid grid-cols-7 gap-2">
          {WIDGET_ROLES.map((role) => (
            <div key={role} data-w-role={role} className="rounded-notch-w-card bg-widget-tile p-2">
              <div className="h-3 rounded-full bg-widget-acc" />
              <div className="mt-1 h-1.5 rounded-full bg-widget-track" />
              <div className="mt-1 text-notch-w-over font-semibold text-widget-acc-ink">{role}</div>
            </div>
          ))}
        </div>
      </Block>
    </div>
  );
}
