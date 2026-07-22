import { commands, type Phase } from "../../lib/bindings";
import { Button } from "@/components/ui/button";
import { Group, PaneTitle, Row } from "@/components/ui/rows";

/** Overlay states in the order a dictation walks through them; `idle` hides
 * it, which is how the preview is dismissed. */
const PHASES: { phase: Phase; label: string; hint: string }[] = [
  { phase: "recording", label: "Recording", hint: "The pill (or wings) with the live level meter." },
  { phase: "transcribing", label: "Transcribing", hint: "The waiting state, after the key is released." },
  { phase: "done", label: "Done", hint: "The confirmation shown as the text lands." },
  { phase: "error", label: "Error", hint: "Forces the pill, whatever the style setting says." },
  { phase: "idle", label: "Hide", hint: "Dismisses the overlay." },
];

/**
 * Draws any overlay state on demand. The overlay otherwise lives on a
 * click-through window that only exists mid-dictation, so checking a spacing
 * change meant holding a key and talking.
 *
 * ponytail: no update-overlay preview. There is nothing to preview until Phase
 * 8 lands a signing keypair and the updater has a UI; the roadmap pairs the two.
 */
export function DebugView() {
  return (
    <div className="flex flex-col gap-5">
      <PaneTitle
        title="Debug"
        subtitle="Draws overlay states without a real dictation. Visible because Developer tools is on."
      />
      <Group title="Overlay">
        {PHASES.map(({ phase, label, hint }) => (
          <Row key={phase} label={label} hint={hint}>
            <Button
              variant="plain"
              size="text"
              onClick={() => commands.debugOverlay(phase)}
            >
              Show
            </Button>
          </Row>
        ))}
      </Group>
    </div>
  );
}
