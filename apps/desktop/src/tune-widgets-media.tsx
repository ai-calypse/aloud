import { useState, type ReactNode } from "react";
import type { BtDevice, Lyrics, NowPlaying, Reading, SoundState } from "./lib/bindings";
import { DevicesBoard } from "./features/notch/views/widgets/Devices";
import { LyricsBoard, keyOf, type Load } from "./features/notch/views/widgets/Lyrics";
import { PlayingBoard } from "./features/notch/views/widgets/Playing";
import type { Controls } from "./features/notch/views/widgets/playback";
import { SoundBoard } from "./features/notch/views/widgets/Sound";

/**
 * FIXTURES — the Media widgets on the tune page. Everything here is made up
 * and exists only to be looked at: every state of the four Media subsections,
 * reachable without Tauri, a player or a Bluetooth device. The app never
 * imports this file. (Compare against the Figma Music, Podcasts and Batteries
 * widgets and docs/design/notch-personality.html.)
 */

const NOOP = () => {};
const CONTROLS: Controls = { toggle: NOOP, previous: NOOP, next: NOOP, jump: NOOP, seek: NOOP, speed: NOOP };

const SONG: NowPlaying = {
  player: "spotify",
  source: { app: "Spotify", host: null, tab: null },
  state: "playing",
  title: "Night Drive",
  artist: "Halcyon",
  album: "Neon",
  durationMs: 200_000,
  positionMs: 42_000,
  speedPct: 100,
  artworkUrl: null,
  song: null,
} as NowPlaying;

const WEB: NowPlaying = {
  ...SONG,
  player: "browser",
  source: { app: "Brave Browser", host: "www.youtube.com", tab: "1/2" },
  title: "Why agents need receipts",
  artist: "Latent Space",
  album: "",
  durationMs: 6_840_000,
  positionMs: 2_130_000,
  speedPct: 125,
} as NowPlaying;

const reading = (now: NowPlaying | null, over: Partial<Reading> = {}): Reading => ({
  now,
  denied: null,
  browsers: [],
  ...over,
});

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

const LINES = [
  "Headlights fold the dark in two",
  "Nothing on the radio but you",
  "",
  "I count the lamps along the bay",
  "Every one a mile away",
  "We were never going anywhere",
  "That was always the way",
  "Turn it up, turn it up",
  "The city is a quiet song",
].map((text, i) => ({ ms: 20_000 + i * 11_000, text }));

const READY = (now: NowPlaying, lyrics: Lyrics | null): Load => ({ key: keyOf(now), status: "ready", lyrics });

const dev = (name: string, kind: string, percent: number[] | null, labels: string[] = []): BtDevice => ({
  name,
  address: name.toLowerCase().replace(/\W/g, ":"),
  kind,
  connected: percent !== null,
  batteries: (percent ?? []).map((p, i) => ({ label: labels[i] ?? "", percent: p })),
});

const DEVICES: BtDevice[] = [
  dev("AirPods Pro", "headphones", [100, 98, 63], ["Left", "Right", "Case"]),
  dev("Magic Mouse", "mouse", [80]),
  dev("Magic Keyboard", "keyboard", [18]),
  dev("Sony WH-1000XM5", "headphones", null),
  dev("Magic Trackpad", "trackpad", []),
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
  const readAt = performance.now();
  const [muted, setMuted] = useState(false);
  const board = (r: Reading, extra: { sound?: SoundState } = {}) => (
    <PlayingBoard
      reading={r}
      readAt={readAt}
      art={null}
      icon={null}
      controls={CONTROLS}
      sound={extra.sound ?? SOUND}
    />
  );
  const lyrics = (now: NowPlaying | null, load: Load | null) => (
    <LyricsBoard now={now} readAt={readAt} load={load} art={null} onToggle={NOOP} onRetry={NOOP} />
  );
  const devices = (loaded: Parameters<typeof DevicesBoard>[0]["loaded"], error: string | null = null) => (
    <DevicesBoard loaded={loaded} connecting={null} error={error} onConnect={NOOP} />
  );
  return (
    <div data-notch-style="widgets" className="grid max-w-[720px] gap-9 font-notch-w">
      <Block name="Media · Now Playing" note="a Spotify track: the large Music widget, where the sound is going beside it">
        {board(reading(SONG))}
      </Block>
      <Block name="Media · Now Playing, web video" note="Podcasts purple, 10 second skips and the speed chip, with a browser that needs a fix beside it">
        {board(reading(WEB, { browsers: [{ app: "Safari", kind: "jsOff" }] }))}
      </Block>
      <Block name="Media · Now Playing, paused, nothing else known" note="paused, and no output devices read yet">
        {board(reading({ ...SONG, state: "paused" }), { sound: { outputs: [], inputs: [], micMuted: null } })}
      </Block>
      <Block name="Media · Now Playing, empty and refused" note="nothing playing; Automation refused for Spotify; browsers that cannot be read">
        <div className="grid gap-6">
          {board(reading(null))}
          {board(reading(null, { denied: "spotify" }))}
          {board(reading(null, { browsers: [{ app: "Brave Browser", kind: "jsOff" }, { app: "Safari", kind: "notPermitted" }] }))}
        </div>
      </Block>

      <Block name="Media · Lyrics, synced" note="the line being sung is bright and the scroll is the live spring; the clock is running, so the lines move">
        {lyrics({ ...SONG, positionMs: 53_000 }, READY(SONG, { lines: LINES, plain: null, instrumental: false }))}
      </Block>
      <Block name="Media · Lyrics, other states" note="loading, the service unreachable, plain text, instrumental, none, a web video with no song, nothing playing">
        <div className="grid gap-6">
          {lyrics(SONG, { key: keyOf(SONG), status: "loading" })}
          {lyrics(SONG, { key: keyOf(SONG), status: "error", message: "The lyrics service did not answer in time." })}
          {lyrics(SONG, READY(SONG, { lines: [], plain: "Headlights fold the dark in two\nNothing on the radio but you", instrumental: false }))}
          {lyrics(SONG, READY(SONG, { lines: [], plain: null, instrumental: true }))}
          {lyrics(SONG, READY(SONG, null))}
          {lyrics(WEB, null)}
          {lyrics(null, null)}
        </div>
      </Block>

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
