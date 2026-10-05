import { useState, type ReactNode } from "react";
import type { BtDevice, SoundState } from "./lib/bindings";
import { DevicesBoard } from "./features/notch/views/widgets/Devices";
import { SoundBoard } from "./features/notch/views/widgets/Sound";

/**
 * FIXTURES — the Media widget boards on the tune page. Everything here is made
 * up and exists only to be looked at: every state of Sound and Devices,
 * reachable without Tauri or a Bluetooth device. The app never imports this
 * file. (Compare against the Figma Batteries widget and
 * docs/design/notch-personality.html.)
 */

const NOOP = () => {};

const SOUND: SoundState = {
  outputs: [
    { id: 1, name: "MacBook Pro Speakers", transport: "Built-in", isDefault: false },
    { id: 2, name: "AirPods Pro", transport: "Bluetooth", isDefault: true },
    { id: 3, name: "LG UltraFine", transport: "HDMI", isDefault: false },
  ],
  inputs: [
    { id: 4, name: "MacBook Pro Microphone", transport: "Built-in", isDefault: true },
    { id: 5, name: "AirPods Pro", transport: "Bluetooth", isDefault: false },
  ],
  micMuted: false,
};

const dev = (
  name: string,
  kind: string,
  level: number | null,
  rest: Partial<BtDevice> = {},
): BtDevice => ({
  name,
  address: name.toLowerCase().replace(/\W/g, ":"),
  kind,
  connected: level !== null,
  batteries: level === null ? [] : [{ label: "", percent: level }],
  level,
  case: null,
  singleBud: null,
  ...rest,
});
const pods = (name: string, left: number | null, right: number | null, caseLevel: number | null): BtDevice => {
  const buds = [left, right].filter((p): p is number => p !== null);
  return dev(name, "headphones", Math.ceil(buds.reduce((a, b) => a + b, 0) / buds.length), {
    batteries: [
      ...(left === null ? [] : [{ label: "Left", percent: left }]),
      ...(right === null ? [] : [{ label: "Right", percent: right }]),
      ...(caseLevel === null ? [] : [{ label: "Case", percent: caseLevel }]),
    ],
    case: caseLevel,
    singleBud: left === null ? "Right" : right === null ? "Left" : null,
  });
};
const DEVICES: BtDevice[] = [
  pods("AirPods Pro", 100, 98, 63),
  dev("Magic Mouse", "mouse", 80),
  dev("Magic Keyboard", "keyboard", 18),
  dev("Sony WH-1000XM5", "headphones", null),
  dev("Magic Trackpad", "trackpad", null, { connected: true }),
  pods("Beats Fit Pro", 41, null, 90),
];
function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

export function MediaWidgetGallery() {
  const [muted, setMuted] = useState(false);
  const devices = (loaded: Parameters<typeof DevicesBoard>[0]["loaded"], error: string | null = null) => (
    <DevicesBoard loaded={loaded} connecting={null} error={error} onConnect={NOOP} />
  );
  return (
    <div className="grid max-w-[720px] gap-9 font-notch-w">


      <Block name="Media · Sound" note="outputs, inputs, the microphone switch, and where the sound goes now">
        <SoundBoard state={{ ...SOUND, micMuted: muted }} error={null} onPick={NOOP} onMute={setMuted} />
      </Block>
      <Block name="Media · Sound, no mic switch, an error, and none" note="a USB interface with no mute; a switch that failed; no audio devices at all">
        <div className="grid gap-6">
          <SoundBoard state={{ ...SOUND, micMuted: null }} error="Could not set the output: device busy." onPick={NOOP} onMute={NOOP} />
          <SoundBoard state={{ outputs: [], inputs: [], micMuted: null }} error={null} onPick={NOOP} onMute={NOOP} />
        </div>
      </Block>

      <Block name="Media · Devices" note="the Batteries family: the list (with Connect for one that is off), four rings, the one with the least left">
        {devices({ devices: DEVICES, error: null })}
      </Block>
      <Block name="Media · Devices, empty, error, looking" note="nothing paired; the profiler failed; first read not back yet">
        <div className="grid gap-6">
          {devices({ devices: [], error: null })}
          {devices({ devices: [], error: "system_profiler did not answer." })}
          {devices(null)}
        </div>
      </Block>
    </div>
  );
}
