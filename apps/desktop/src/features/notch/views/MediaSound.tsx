import { useState } from "react";
import { CheckIcon } from "lucide-react";
import { commands, type Device, type Direction, type SoundState } from "@/lib/bindings";
import { Switch } from "@/components/notch/controls";
import { Empty, Label, Lead, List, Row } from "@/components/notch/list";
import { usePolled } from "../media";

const NONE: SoundState = { outputs: [], inputs: [], micMuted: null };
/** Two seconds: a pair of headphones connecting should show up while you watch. */
const POLL_MS = 2000;

/**
 * Media › Sound: where audio goes and comes from, and the microphone switch.
 * The truth is CoreAudio's, so the list is re-read on a short clock and after
 * every change rather than kept here.
 */
export function MediaSound() {
  const [state, refresh] = usePolled(() => commands.soundState(), POLL_MS, NONE);
  const [error, setError] = useState<string | null>(null);

  const settle = (result: Promise<{ status: "ok" } | { status: "error"; error: string }>) => {
    result
      .then((res) => setError(res.status === "error" ? res.error : null))
      .catch(() => {})
      .finally(refresh);
  };
  const pick = (direction: Direction, device: Device) => {
    if (device.isDefault) return;
    try {
      settle(commands.soundSetDefault(direction, device.id));
    } catch {
      /* browser preview */
    }
  };

  const output = state.outputs.find((d) => d.isDefault);
  if (state.outputs.length === 0 && state.inputs.length === 0) {
    return (
      <Empty title="No audio devices">
        Speakers, headphones and microphones appear here as they connect.
      </Empty>
    );
  }

  return (
    <>
      {output && (
        <Lead
          className="mb-3"
          sub={`Playing through ${output.transport ? output.transport.toLowerCase() : "this Mac"}`}
        >
          <span className="block truncate">{output.name}</span>
        </Lead>
      )}
      <Label>Output</Label>
      <DeviceList label="Output devices" devices={state.outputs} onPick={(d) => pick("output", d)} />
      <Label className="mt-3">Input</Label>
      <DeviceList label="Input devices" devices={state.inputs} onPick={(d) => pick("input", d)} />
      {state.micMuted !== null && (
        <>
          <Label className="mt-3">Microphone</Label>
          <List label="Microphone">
            <Row
              title="Mute microphone"
              subtitle="Silences every app at once, meetings included"
              trailing={
                <Switch
                  label="Mute microphone"
                  checked={state.micMuted}
                  onChange={(muted) => {
                    try {
                      settle(commands.soundSetMicMuted(muted));
                    } catch {
                      /* browser preview */
                    }
                  }}
                />
              }
            />
          </List>
        </>
      )}
      {error && <p className="m-0 mt-2 font-notch text-notch-meta text-notch-ink-3">{error}</p>}
    </>
  );
}

function DeviceList({
  label,
  devices,
  onPick,
}: {
  label: string;
  devices: Device[];
  onPick: (device: Device) => void;
}) {
  return (
    <List label={label}>
      {devices.map((device) => (
        <Row
          key={device.id}
          selected={device.isDefault}
          onClick={() => onPick(device)}
          lead={
            <span className="grid size-[26px] shrink-0 place-items-center text-notch-ink">
              {device.isDefault && <CheckIcon className="size-3.5" aria-hidden />}
            </span>
          }
          title={device.name}
          subtitle={device.transport || undefined}
        />
      ))}
    </List>
  );
}
