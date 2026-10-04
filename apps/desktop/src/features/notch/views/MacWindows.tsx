import { useState } from "react";
import { commands, type Snap } from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { Empty, Label, Lead } from "@/components/notch/list";
import { usePoll } from "../usePoll";
import { SNAPS } from "../snaps";

/** Slow on purpose: the front window changes when the user moves, not while they read. */
const POLL_MS = 2000;

const GROUPS = ["Halves", "Quarters", "Whole"] as const;

/**
 * Mac › Windows: snap the window the user was in. The buttons name the
 * result, not a glyph to learn; Maximize is the view's one primary.
 */
export function MacWindows() {
  const front = usePoll(() => commands.macFrontWindow(), POLL_MS);
  const [said, setSaid] = useState("");

  if (!front) return null;

  if (!front.trusted) {
    return (
      <Empty
        title="Snapping needs Accessibility"
        action={
          <Button
            variant="notch"
            size="notch"
            onClick={() => void commands.openPermissionSettings("accessibility")}
          >
            Open Accessibility settings
          </Button>
        }
      >
        macOS only lets an app move other apps' windows once Accessibility is
        allowed for it in Privacy &amp; Security.
      </Empty>
    );
  }

  const snap = async (id: Snap) => {
    const res = await commands.macSnap(id);
    setSaid(res.status === "ok" ? `Moved ${res.data}` : res.error);
  };

  return (
    <div className="grid grid-cols-[1fr_1.4fr] gap-6 font-notch">
      <div className="min-w-0">
        <Label>Front window</Label>
        <Lead className="break-words" sub={front.app ? "Snap it to part of the screen" : "Open a window to snap it"}>
          {front.app ?? "No window"}
        </Lead>
        <p className="m-0 mt-2 min-h-[18px] text-notch-meta text-notch-ink-3" aria-live="polite">
          {said}
        </p>
      </div>
      <div className="grid gap-3">
        {GROUPS.map((group) => (
          <div key={group}>
            <Label>{group}</Label>
            <div className="flex flex-wrap gap-1.5">
              {SNAPS.filter((s) => s.group === group).map((s) => (
                <Button
                  key={s.id}
                  variant={s.id === "maximize" ? "notch" : "notch-quiet"}
                  size="notch"
                  disabled={!front.app}
                  className={s.id === "maximize" ? undefined : "border border-notch-edge"}
                  onClick={() => void snap(s.id)}
                >
                  {s.label}
                </Button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
