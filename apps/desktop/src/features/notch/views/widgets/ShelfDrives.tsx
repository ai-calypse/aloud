import { useState } from "react";
import { HardDriveIcon } from "lucide-react";
import { Ring, WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { commands, type ShelfDrive } from "@/lib/bindings";
import { useDropToShelf, useNote, useShelfList, useShelfPrefs } from "../shelf-data";
import { InfoTile, Note, Pill, WatchTile } from "./shelf-parts";
import { driveRoom, driveUsage } from "./shelf-widgets";

/**
 * Shelf › Drives as a widget board: one tile per external drive or mounted
 * disk image, its fullness a ring (green, amber, red, always with the
 * percentage and a word), and Eject. A drive that is in use says what is
 * holding it rather than failing quietly.
 */
export function WidgetShelfDrives() {
  const { prefs, set } = useShelfPrefs();
  const [note, setNote] = useNote(10000);
  useDropToShelf(setNote);
  const [ejecting, setEjecting] = useState<string | null>(null);
  const { items } = useShelfList<ShelfDrive>(
    async () => {
      const r = await commands.drivesList();
      if (r.status === "error") throw new Error(r.error);
      return r.data;
    },
    "drives",
  );

  async function eject(drive: ShelfDrive) {
    setEjecting(drive.mount);
    setNote(null);
    const r = await commands.driveEject(drive.mount);
    setEjecting(null);
    setNote(r.status === "ok" ? `${drive.name} is safe to disconnect` : r.error);
  }

  return (
    <>
      <DrivesBoard
        drives={items ?? []}
        ejecting={ejecting}
        onEject={(drive) => void eject(drive)}
        watch={prefs ? { on: prefs.drives !== false, onSet: set } : null}
      />
      {note && <Note>{note}</Note>}
    </>
  );
}

export function DrivesBoard({
  drives,
  ejecting,
  onEject,
  watch,
}: {
  drives: ShelfDrive[];
  ejecting: string | null;
  onEject: (drive: ShelfDrive) => void;
  watch: { on: boolean; onSet: Parameters<typeof WatchTile>[0]["onSet"] } | null;
}) {
  return (
    <WidgetBoard label="Drives">
      {drives.length === 0 ? (
        <InfoTile label="Drives" icon={<HardDriveIcon strokeWidth={1.6} />} title="No external drives">
          Plug in a USB drive or SD card, or mount a disk image, and it shows up here with an Eject button.
        </InfoTile>
      ) : (
        drives.map((drive) => {
          const u = driveUsage(drive);
          return (
            <WidgetFrame key={drive.mount} size="medium" label={drive.name} accent={u.accent} ground="vibrant">
              <div className="flex h-full items-center gap-4 p-3.5">
                <Ring
                  value={u.value}
                  label={`${drive.name} fullness`}
                  valueText={u.percent === null ? "Size unknown" : `${u.percent}% full${u.word ? `, ${u.word}` : ""}`}
                  size={96}
                  stroke={12}
                >
                  <HardDriveIcon aria-hidden className="size-6 text-widget-ink" strokeWidth={1.8} />
                </Ring>
                <div className="flex min-w-0 flex-1 flex-col gap-0.5 leading-tight">
                  <span className="truncate text-notch-w-headline font-semibold">{drive.name}</span>
                  {drive.bus && <span className="text-notch-w-meta text-widget-ink-2">{drive.bus}</span>}
                  <span className="text-notch-w-title">{driveRoom(drive)}</span>
                  {u.percent !== null && (
                    <span className="text-notch-w-meta font-medium text-widget-acc-ink">
                      {u.percent}% full{u.word && ` · ${u.word}`}
                    </span>
                  )}
                  {/* The one primary only when there is one thing to eject. */}
                  <Pill
                    tone={drives.length === 1 ? "primary" : "quiet"}
                    className="mt-1.5 w-fit"
                    disabled={ejecting !== null}
                    onClick={() => onEject(drive)}
                  >
                    {ejecting === drive.mount ? "Ejecting…" : "Eject"}
                  </Pill>
                </div>
              </div>
            </WidgetFrame>
          );
        })
      )}

      {watch && (
        <WatchTile
          which="drives"
          on={watch.on}
          onSet={watch.onSet}
          icon={<HardDriveIcon strokeWidth={1.6} />}
          title="Drive alerts"
          subtitle="Drop a chip from the notch when a drive is connected"
        />
      )}
    </WidgetBoard>
  );
}
