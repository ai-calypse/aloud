import { useState } from "react";
import {
  AppWindowIcon,
  ArrowDownIcon,
  ArrowUpIcon,
  BatteryChargingIcon,
  BatteryIcon,
  CameraIcon,
  EyeIcon,
  HeartPulseIcon,
  LaptopIcon,
  MicIcon,
  RefreshCwIcon,
  ThermometerIcon,
} from "lucide-react";
import { NOTCH_FOCUS } from "@/components/ui/button";
import {
  Numeral,
  Ring,
  RingStack,
  WidgetBoard,
  WidgetFrame,
} from "@/components/notch/widgets";
import {
  commands,
  type Battery,
  type FrontWindowInfo,
  type PrivacyUse,
  type Snap,
  type SystemStats,
} from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { usePoll } from "../../usePoll";
import { useWatcher } from "../../useWatcher";
import { Idle, Pill, WidgetSwitch } from "./parts";
import {
  GRID_SNAPS,
  SNAP_REGION,
  WHOLE_SNAPS,
  powerWidget,
  privacyTile,
  rateParts,
  statRings,
  thermalWidget,
} from "./mac-widgets";

/* Mac › Stats ------------------------------------------------------------ */

/** Two seconds: a number that moves, without the Mac spending its own load drawing it. */
const STATS_POLL_MS = 2000;

/**
 * Mac › Stats in the widgets look: Fitness's three rings for CPU, Memory and
 * Disk, with network and heat beside them. Polls only while mounted, like the
 * calm view: `usePoll` lives and dies with the view.
 */
export function WidgetMacStats() {
  const stats = usePoll(async () => {
    const res = await commands.macStats();
    if (res.status === "error") throw new Error(res.error);
    return res.data;
  }, STATS_POLL_MS);
  return <StatsBoard stats={stats} />;
}

export function StatsBoard({ stats }: { stats: SystemStats | undefined }) {
  if (!stats) {
    return (
      <WidgetBoard label="System">
        <WidgetFrame size="medium" label="System" ground="deep">
          <Idle icon={<LaptopIcon strokeWidth={1.6} />}>
            Reading the Mac. CPU, memory, disk and network show up a moment after the panel opens.
          </Idle>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  const rings = statRings(stats);
  const heat = thermalWidget(stats.thermal);
  const up = rateParts(stats.netUp);
  const down = rateParts(stats.netDown);
  return (
    <WidgetBoard label="System">
      <WidgetFrame size="medium" label="System" ground="deep">
        <div className="flex h-full items-center gap-4 p-3.5">
          <RingStack
            size={104}
            label="System load"
            rings={rings.map((r) => ({
              value: r.value,
              label: r.label,
              accent: r.accent,
              valueText: `${r.text}%, ${r.sub}`,
            }))}
          />
          <div className="flex min-w-0 flex-1 flex-col gap-2 font-notch-w-rounded">
            {rings.map((r) => (
              // The role lives on the row so its words take that ring's ink.
              <div key={r.key} data-w-role={r.accent} className="flex items-center gap-2">
                <span className="flex w-18 items-baseline">
                  <Numeral text={r.text} size="s" className="text-widget-acc-ink" />
                  <span className="text-notch-w-title font-medium text-widget-acc-ink">%</span>
                </span>
                <span className="min-w-0 leading-tight">
                  <span className="block text-notch-w-over font-semibold tracking-wide text-widget-acc-ink uppercase">
                    {r.label}
                  </span>
                  <span className="block truncate text-notch-w-meta text-widget-ink-2">{r.sub}</span>
                </span>
              </div>
            ))}
          </div>
        </div>
      </WidgetFrame>
      <WidgetFrame size="small" label="Network" accent="blue">
        <div className="flex h-full flex-col justify-between p-3.5">
          <Rate icon={<ArrowUpIcon aria-hidden className="size-3.5" strokeWidth={2.4} />} name="Up" {...up} />
          <Rate icon={<ArrowDownIcon aria-hidden className="size-3.5" strokeWidth={2.4} />} name="Down" {...down} />
        </div>
      </WidgetFrame>
      <WidgetFrame size="small" label="Heat" accent={heat.accent} live={heat.hot}>
        <div className="flex h-full flex-col justify-between p-3.5">
          <ThermometerIcon aria-hidden className="size-6 text-widget-acc-ink" strokeWidth={1.8} />
          <div>
            <div className="text-notch-w-headline leading-tight font-semibold text-widget-acc-ink">
              {heat.word}
            </div>
            <div className="text-notch-w-title font-medium text-widget-ink-2">Thermal pressure</div>
          </div>
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}

function Rate({
  icon,
  name,
  value,
  unit,
}: {
  icon: React.ReactNode;
  name: string;
  value: string;
  unit: string;
}) {
  return (
    <div>
      <span className="flex items-center gap-1 text-notch-w-over font-semibold tracking-wide text-widget-acc-ink uppercase">
        {icon}
        {name}
      </span>
      <span className="flex items-baseline gap-1">
        <Numeral text={value} size="s" />
        <span className="text-notch-w-meta font-medium text-widget-ink-2">{unit}</span>
      </span>
    </div>
  );
}

/* Mac › Power ------------------------------------------------------------ */

/** The estimate moves in minutes; ten seconds keeps a plug-in noticed promptly. */
const POWER_POLL_MS = 10_000;

/** Mac › Power in the widgets look: the Batteries widget, with its watcher switch kept on the tile. */
export function WidgetMacPower() {
  // `null` is a Mac with no battery; `undefined` is not read yet.
  const battery = usePoll(() => commands.macBattery(), POWER_POLL_MS);
  const [alerts, setAlerts] = useWatcher("battery");
  return <PowerBoard battery={battery} alerts={alerts} onAlerts={setAlerts} />;
}

export function PowerBoard({
  battery,
  alerts,
  onAlerts,
}: {
  battery: Battery | null | undefined;
  alerts: boolean | null;
  onAlerts: (on: boolean) => void;
}) {
  if (!battery) {
    return (
      <WidgetBoard label="Battery">
        <WidgetFrame size="medium" label="Battery" ground="vibrant">
          <Idle icon={<BatteryIcon strokeWidth={1.6} />}>
            {battery === null
              ? "No battery in this Mac. A desktop Mac has none to read; this fills in on a MacBook."
              : "Reading the battery. The charge shows up a moment after the panel opens."}
          </Idle>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  const p = powerWidget(battery);
  const Icon = p.charging ? BatteryChargingIcon : BatteryIcon;
  return (
    <WidgetBoard label="Battery">
      <WidgetFrame size="medium" label="Battery" accent={p.accent} ground="vibrant">
        <div className="flex h-full gap-4 p-3.5">
          <Ring
            value={p.value}
            label="Battery charge"
            valueText={`${p.text}%, ${p.word}`}
            size={104}
            stroke={12}
            className="self-center"
          >
            <Icon aria-hidden className="size-7 text-widget-ink" strokeWidth={1.8} />
          </Ring>
          <div className="flex min-w-0 flex-1 flex-col justify-between">
            <div>
              <div className="flex items-baseline">
                <Numeral text={p.text} size="l" roll />
                <span className="text-notch-w-numeral-s leading-none font-light text-widget-ink">%</span>
              </div>
              <div className="text-notch-w-title leading-snug font-medium text-widget-ink-2">
                {p.sentence}
              </div>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className="min-w-0 text-notch-w-meta leading-tight font-medium text-widget-ink-2">
                Alert me at full and below 20%
              </span>
              <WidgetSwitch
                checked={alerts ?? true}
                disabled={alerts === null}
                onChange={onAlerts}
                label="Battery alerts"
              />
            </div>
          </div>
        </div>
      </WidgetFrame>
      {p.health && (
        <WidgetFrame size="small" label="Capacity" accent="ok" ground="vibrant">
          <div className="flex h-full flex-col justify-between p-3.5">
            <Ring
              value={p.health.value}
              label="Battery capacity"
              valueText={`${p.health.text}% of when it was new`}
              size={48}
              stroke={8}
            >
              <HeartPulseIcon aria-hidden className="size-5 text-widget-ink" strokeWidth={2} />
            </Ring>
            <div>
              <div className="flex items-baseline">
                <Numeral text={p.health.text} size="l" />
                <span className="text-notch-w-numeral-s leading-none font-light text-widget-ink">%</span>
              </div>
              <div className="text-notch-w-title font-medium text-widget-ink-2">Compared with new</div>
            </div>
          </div>
        </WidgetFrame>
      )}
      {p.cycles && (
        <WidgetFrame size="small" label="Cycles" ground="vibrant">
          <div className="flex h-full flex-col justify-between p-3.5">
            <RefreshCwIcon aria-hidden className="size-6 text-widget-ink-2" strokeWidth={1.8} />
            <div>
              <Numeral text={p.cycles} size="l" />
              <div className="text-notch-w-title font-medium text-widget-ink-2">Charge cycles</div>
            </div>
          </div>
        </WidgetFrame>
      )}
    </WidgetBoard>
  );
}

/* Mac › Privacy ---------------------------------------------------------- */

const PRIVACY_POLL_MS = 1000;

/**
 * Mac › Privacy in the widgets look: a tile each for the microphone and the
 * camera, lit when in use, and the tile that carries the watcher switch.
 */
export function WidgetMacPrivacy() {
  const use = usePoll(() => commands.macPrivacy(), PRIVACY_POLL_MS);
  const [watching, setWatching] = useWatcher("privacy");
  return <PrivacyBoard use={use} watching={watching} onWatching={setWatching} />;
}

export function PrivacyBoard({
  use,
  watching,
  onWatching,
}: {
  use: PrivacyUse | undefined;
  watching: boolean | null;
  onWatching: (on: boolean) => void;
}) {
  return (
    <WidgetBoard label="Mic and camera">
      {use ? (
        <>
          <PrivacyTile
            title="Microphone"
            icon={<MicIcon aria-hidden className="size-4.5 text-widget-ink" strokeWidth={2} />}
            accent="orange"
            {...privacyTile("mic", use.mic, use.micApps)}
          />
          <PrivacyTile
            title="Camera"
            icon={<CameraIcon aria-hidden className="size-4.5 text-widget-ink" strokeWidth={2} />}
            accent="ok"
            {...privacyTile("camera", use.camera, [])}
          />
        </>
      ) : (
        <WidgetFrame size="medium" label="Mic and camera">
          <Idle icon={<MicIcon strokeWidth={1.6} />}>
            Checking the mic and camera. What is using them shows up a moment after the panel opens.
          </Idle>
        </WidgetFrame>
      )}
      <WidgetFrame size="small" label="Indicator">
        <div className="flex h-full flex-col justify-between p-3.5">
          <div className="flex items-start justify-between gap-2">
            <EyeIcon aria-hidden className="size-6 text-widget-ink-2" strokeWidth={1.6} />
            <WidgetSwitch
              checked={watching ?? true}
              disabled={watching === null}
              onChange={onWatching}
              label="Show mic and camera use"
            />
          </div>
          <div>
            <div className="text-notch-w-title leading-tight font-semibold">Show beside the camera</div>
            <div className="text-notch-w-meta leading-tight text-widget-ink-2">
              While the notch is closed
            </div>
          </div>
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}

function PrivacyTile({
  title,
  icon,
  accent,
  on,
  word,
  detail,
}: {
  title: string;
  icon: React.ReactNode;
  accent: "orange" | "ok";
  on: boolean;
  word: string;
  detail: string;
}) {
  return (
    // A hue only while it is on; off is the neutral tile, so the colour is a
    // second signal beside the word, never the only one.
    <WidgetFrame size="small" label={title} accent={on ? accent : undefined} live={on}>
      <div className="flex h-full flex-col justify-between p-3.5">
        <Ring
          value={on ? 1 : 0}
          label={title}
          valueText={word}
          size={44}
          stroke={7}
          motion="none"
        >
          {icon}
        </Ring>
        <div>
          <div className="text-notch-w-headline leading-tight font-semibold">{word}</div>
          <div className="line-clamp-2 text-notch-w-meta leading-tight font-medium text-widget-ink-2">
            {detail}
          </div>
        </div>
      </div>
    </WidgetFrame>
  );
}

/* Mac › Windows ---------------------------------------------------------- */

/** Slow on purpose: the front window changes when the user moves, not while they read. */
const WINDOWS_POLL_MS = 2000;

/** Mac › Windows in the widgets look: a snap grid of miniature screens. */
export function WidgetMacWindows() {
  const front = usePoll(() => commands.macFrontWindow(), WINDOWS_POLL_MS);
  const [said, setSaid] = useState("");
  const snap = async (id: Snap) => {
    const res = await commands.macSnap(id);
    setSaid(res.status === "ok" ? `Moved ${res.data}` : res.error);
  };
  return (
    <WindowsBoard
      front={front}
      said={said}
      onSnap={(id) => void snap(id)}
      onOpenSettings={() => void commands.openPermissionSettings("accessibility")}
    />
  );
}

export function WindowsBoard({
  front,
  said,
  onSnap,
  onOpenSettings,
}: {
  front: FrontWindowInfo | undefined;
  said: string;
  onSnap: (id: Snap) => void;
  onOpenSettings: () => void;
}) {
  // Like the calm view: nothing while the first read is on its way.
  if (!front) return null;

  if (!front.trusted) {
    return (
      <WidgetBoard label="Windows">
        <WidgetFrame size="medium" label="Windows" accent="blue">
          <div className="flex h-full flex-col justify-between p-3.5">
            <div>
              <div className="text-notch-w-headline font-semibold">Snapping needs Accessibility</div>
              <div className="mt-1 text-notch-w-title leading-snug text-widget-ink-2">
                macOS only lets an app move other apps&apos; windows once Accessibility is allowed for
                it in Privacy &amp; Security.
              </div>
            </div>
            <Pill primary className="self-start" onClick={onOpenSettings}>
              Open Accessibility settings
            </Pill>
          </div>
        </WidgetFrame>
      </WidgetBoard>
    );
  }

  const off = !front.app;
  return (
    <WidgetBoard label="Windows">
      <WidgetFrame size="small" label="Front window" accent="blue">
        <div className="flex h-full flex-col justify-between p-3.5">
          <AppWindowIcon aria-hidden className="size-6 text-widget-acc-ink" strokeWidth={1.6} />
          <div className="min-w-0">
            <div className="line-clamp-2 text-notch-w-headline leading-tight font-semibold break-words">
              {front.app ?? "No window"}
            </div>
            {/* What just happened replaces the hint: both are one polite line. */}
            <p className="m-0 line-clamp-2 text-notch-w-meta leading-tight text-widget-ink-2" aria-live="polite">
              {said || (front.app ? "Snap it to part of the screen" : "Open a window to snap it")}
            </p>
          </div>
        </div>
      </WidgetFrame>
      <WidgetFrame size="medium" label="Snap" accent="blue">
        <div className="grid h-full grid-cols-4 grid-rows-2 gap-1.5 p-2.5">
          {GRID_SNAPS.map((s) => (
            <SnapButton key={s.id} id={s.id} label={s.label} disabled={off} onSnap={onSnap} />
          ))}
        </div>
      </WidgetFrame>
      <WidgetFrame size="small" label="Whole window" accent="blue">
        <div className="flex h-full flex-col justify-center gap-2 p-3.5">
          {WHOLE_SNAPS.map((s) => (
            <Pill
              key={s.id}
              primary={s.id === "maximize"}
              disabled={off}
              className="h-9 w-full"
              onClick={() => onSnap(s.id)}
            >
              <SnapGlyph id={s.id} />
              {s.label}
            </Pill>
          ))}
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}

/** The screen in miniature, with the part a snap fills. Decoration: the label names it. */
function SnapGlyph({ id }: { id: Snap }) {
  return (
    <span
      aria-hidden
      className="relative block h-4.5 w-7 shrink-0 overflow-hidden rounded-sm border border-widget-ink-3 bg-widget-neutral-track"
    >
      <span className={cn("absolute bg-widget-acc", SNAP_REGION[id])} />
    </span>
  );
}

function SnapButton({
  id,
  label,
  disabled,
  onSnap,
}: {
  id: Snap;
  label: string;
  disabled: boolean;
  onSnap: (id: Snap) => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={() => onSnap(id)}
      className={cn(
        NOTCH_FOCUS,
        "flex min-w-0 flex-col items-center justify-start gap-1.5 rounded-notch-row bg-widget-tile-2 px-1 pt-3",
        "transition-[background-color,transform] duration-[var(--dur-fast)] ease-[var(--ease-out)]",
        "hover:bg-widget-neutral-track active:scale-97 disabled:pointer-events-none disabled:opacity-40",
      )}
    >
      <SnapGlyph id={id} />
      <span className="text-center text-notch-w-over leading-tight font-semibold text-widget-ink">
        {label}
      </span>
    </button>
  );
}
