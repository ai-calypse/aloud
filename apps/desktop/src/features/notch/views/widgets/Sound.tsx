import { useState } from "react";
import { CheckIcon, HeadphonesIcon, MicIcon, MicOffIcon, Volume2Icon } from "lucide-react";
import { commands, type Device, type Direction, type SoundState } from "@/lib/bindings";
import { WidgetBoard, WidgetFrame, type WidgetSize } from "@/components/notch/widgets";
import { NOTCH_FOCUS } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { usePolled } from "../../media";
import { micWord, outputLine } from "./media-format";
import { Notice } from "./parts";

const NONE: SoundState = { outputs: [], inputs: [], micMuted: null };
/** Two seconds: a pair of headphones connecting should show up while you watch. */
const POLL_MS = 2000;

type Done = { status: "ok" } | { status: "error"; error: string };

/**
 * Media › Sound in the widgets look: the output devices as a list tile, the
 * inputs beside it, the microphone as a switch tile and where the sound is
 * going now. CoreAudio is the truth, so the lists are re-read on a short clock
 * and after every change rather than kept here.
 */
export function WidgetMediaSound() {
  const [state, refresh] = usePolled(() => commands.soundState(), POLL_MS, NONE);
  const [error, setError] = useState<string | null>(null);

  const settle = (result: Promise<Done>) => {
    result
      .then((res) => setError(res.status === "error" ? res.error : null))
      .catch(() => {})
      .finally(refresh);
  };
  return (
    <SoundBoard
      state={state}
      error={error}
      onPick={(direction, device) => {
        if (device.isDefault) return;
        try {
          settle(commands.soundSetDefault(direction, device.id));
        } catch {
          /* browser preview */
        }
      }}
      onMute={(muted) => {
        try {
          settle(commands.soundSetMicMuted(muted));
        } catch {
          /* browser preview */
        }
      }}
    />
  );
}

export function SoundBoard({
  state,
  error,
  onPick,
  onMute,
}: {
  state: SoundState;
  error: string | null;
  onPick: (direction: Direction, device: Device) => void;
  onMute: (muted: boolean) => void;
}) {
  if (state.outputs.length === 0 && state.inputs.length === 0) {
    return (
      <WidgetBoard label="Sound">
        <WidgetFrame size="medium" label="Sound">
          <Notice icon={<Volume2Icon strokeWidth={1.6} />} title="No audio devices">
            Speakers, headphones and microphones appear here as they connect.
          </Notice>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  return (
    <WidgetBoard label="Sound">
      <DeviceTile size="large" label="Output" devices={state.outputs} onPick={(d) => onPick("output", d)} />
      <DeviceTile size="medium" label="Input" devices={state.inputs} onPick={(d) => onPick("input", d)} />
      {state.micMuted !== null && <MicTile muted={state.micMuted} onChange={onMute} />}
      {error ? (
        <WidgetFrame size="small" label="Sound">
          <Notice icon={<Volume2Icon strokeWidth={1.6} />} title="Could not switch">
            <p className="m-0">{error}</p>
          </Notice>
        </WidgetFrame>
      ) : (
        <RouteTile size="small" sound={state} />
      )}
    </WidgetBoard>
  );
}

/**
 * Where the sound is going now: the default output and what it is plugged in
 * as. Medium beside Now Playing, where it opens Sound; small here, where the
 * list is already on screen.
 */
export function RouteTile({
  size,
  sound,
  onOpen,
}: {
  size: Extract<WidgetSize, "small" | "medium">;
  sound: SoundState;
  onOpen?: () => void;
}) {
  const output = sound.outputs.find((d) => d.isDefault);
  if (!output) {
    return (
      <WidgetFrame size={size} label="Sound" onActivate={onOpen}>
        <Notice icon={<Volume2Icon strokeWidth={1.6} />} title="No output">
          <p className="m-0">Speakers and headphones appear here as they connect.</p>
        </Notice>
      </WidgetFrame>
    );
  }
  const Glyph = output.transport === "Bluetooth" ? HeadphonesIcon : Volume2Icon;
  return (
    <WidgetFrame size={size} label="Sound" accent="info" live onActivate={onOpen}>
      <div className="flex h-full flex-col justify-between p-3.5 text-widget-acc-ink">
        <Glyph aria-hidden className="size-6" strokeWidth={1.6} />
        <div className="min-w-0">
          <div className="truncate text-notch-w-headline leading-tight font-semibold">{output.name}</div>
          <div className="truncate text-notch-w-meta font-medium text-widget-ink-2">{outputLine(output)}</div>
        </div>
      </div>
    </WidgetFrame>
  );
}

/**
 * The devices of one direction as the Batteries large list is drawn: a row
 * each on a hairline, the current one ticked. Choosing the one already
 * chosen does nothing.
 */
function DeviceTile({
  size,
  label,
  devices,
  onPick,
}: {
  size: WidgetSize;
  label: string;
  devices: Device[];
  onPick: (device: Device) => void;
}) {
  return (
    <WidgetFrame size={size} label={label} accent="info" ground="vibrant">
      {devices.length === 0 ? (
        <Notice icon={<MicIcon strokeWidth={1.6} />} title={`No ${label.toLowerCase()} devices`} />
      ) : (
        <div role="radiogroup" aria-label={`${label} devices`} className="flex h-full flex-col overflow-y-auto px-3.5">
          {devices.map((device) => (
            <button
              key={device.id}
              type="button"
              role="radio"
              aria-checked={device.isDefault}
              onClick={() => onPick(device)}
              className={cn(
                NOTCH_FOCUS,
                "flex min-h-11 w-full shrink-0 items-center gap-3 border-b border-widget-sep text-left last:border-b-0",
              )}
            >
              <span
                aria-hidden
                className={cn(
                  "grid size-5 shrink-0 place-items-center rounded-full",
                  device.isDefault ? "bg-widget-fill text-widget-acc-ink" : "border border-widget-radio",
                )}
              >
                {device.isDefault && <CheckIcon className="size-3" strokeWidth={3} />}
              </span>
              <span className="min-w-0 flex-1">
                <span
                  className={cn(
                    "block truncate text-notch-w-body leading-tight",
                    device.isDefault ? "font-semibold" : "font-medium text-widget-ink-2",
                  )}
                >
                  {device.name}
                </span>
                {device.transport && (
                  <span className="block truncate text-notch-w-meta leading-tight text-widget-ink-2">
                    {device.transport}
                  </span>
                )}
              </span>
            </button>
          ))}
        </div>
      )}
    </WidgetFrame>
  );
}

/**
 * The microphone switch as a whole tile: pressing anywhere on it mutes every
 * app at once, meetings included. Muted is the state worth seeing, so it is the
 * one that fills the tile red, always with its word.
 */
export function MicTile({ muted, onChange }: { muted: boolean; onChange: (muted: boolean) => void }) {
  const Glyph = muted ? MicOffIcon : MicIcon;
  return (
    <WidgetFrame size="small" label="Microphone" accent={muted ? "move" : undefined} ground={muted ? "accent" : "tile"}>
      <button
        type="button"
        role="switch"
        aria-checked={muted}
        aria-label="Mute microphone"
        onClick={() => onChange(!muted)}
        className={cn(NOTCH_FOCUS, "flex size-full flex-col justify-between p-3.5 text-left")}
      >
        <span className="flex items-start justify-between">
          <Glyph aria-hidden className="size-6" strokeWidth={1.6} />
          <span
            aria-hidden
            className={cn(
              "flex h-7 w-12 shrink-0 items-center rounded-full p-0.5 transition-colors duration-[var(--dur-base)] ease-[var(--ease-out)]",
              muted ? "bg-widget-ink/30" : "bg-widget-neutral-track",
            )}
          >
            <span
              className={cn(
                "size-6 rounded-full bg-widget-ink transition-transform duration-[var(--dur-base)] ease-[var(--ease-out)]",
                muted && "translate-x-5",
              )}
            />
          </span>
        </span>
        <span>
          <span className="block text-notch-w-headline leading-tight font-semibold">{micWord(muted)}</span>
          <span className={cn("block text-notch-w-meta leading-tight font-medium", muted ? "text-widget-ink/85" : "text-widget-ink-2")}>
            Silences every app
          </span>
        </span>
      </button>
    </WidgetFrame>
  );
}
