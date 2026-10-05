import { CameraIcon, ClipboardIcon, CopyIcon, CropIcon, ImageIcon, SearchIcon } from "lucide-react";
import { WidgetBoard, WidgetFrame } from "@/components/notch/widgets";
import { commands, type ScreenShot } from "@/lib/bindings";
import { ago, dragOut } from "../Shelf";
import { formatAgo, useDropToShelf, useNote, useShelfList, useShelfPrefs } from "../shelf-data";
import { Chip, InfoTile, Note, Pill, ShelfThumb, TurnOnTile, WatchTile } from "./shelf-parts";
import { splitShots } from "./shelf-widgets";

const capture = (mode: "region" | "clipboard") => void commands.shotsCapture(mode);

/**
 * Shelf › Screenshots in the widgets look: a Photos-style grid. The newest
 * screenshot leads with Copy and Show; the rest are picture tiles with the
 * same two controls. Every picture drags out as a real file, and the two
 * capture tiles are the calm view's buttons. The watcher is off until the
 * user turns it on (macOS asks once to let Aloud see the folder).
 */
export function WidgetShelfShots() {
  const { prefs, set } = useShelfPrefs();
  const [note, setNote] = useNote();
  useDropToShelf(setNote);
  const on = prefs?.shots ?? false;
  const { items } = useShelfList<ScreenShot>(
    async () => {
      const r = await commands.shotsRecent(12);
      if (r.status === "error") throw new Error(r.error);
      return r.data;
    },
    "shots",
    on,
  );

  if (prefs && !on) {
    return (
      <WidgetBoard label="Screenshots">
        <TurnOnTile
          label="Screenshots"
          icon={<CameraIcon strokeWidth={1.6} />}
          which="shots"
          onSet={set}
          title="Screenshot shelf is off"
        >
          Turn it on and each new screenshot drops from the notch, ready to copy, show in Finder or drag into another
          app. macOS will ask once to let Aloud see the folder your screenshots go to.
        </TurnOnTile>
      </WidgetBoard>
    );
  }

  async function copy(path: string) {
    const r = await commands.shotsCopy(path);
    setNote(r.status === "ok" ? "Copied" : r.error);
  }

  return (
    <>
      <ShotsBoard
        shots={items ?? []}
        onCopy={(path) => void copy(path)}
        watch={prefs ? { on, onSet: set } : null}
      />
      {note && <Note>{note}</Note>}
    </>
  );
}

export function ShotsBoard({
  shots,
  onCopy,
  watch,
}: {
  shots: ScreenShot[];
  onCopy: (path: string) => void;
  /** Null until the prefs are read: no tile that could show the wrong switch. */
  watch: { on: boolean; onSet: Parameters<typeof WatchTile>[0]["onSet"] } | null;
}) {
  const { latest, rest } = splitShots(shots);
  return (
    <WidgetBoard label="Screenshots">
      {latest ? (
        <WidgetFrame size="medium" label="Latest screenshot">
          <div className="flex h-full" {...dragOut(latest.path)}>
            <ShelfThumb
              path={latest.path}
              image
              glyph={<ImageIcon strokeWidth={1.4} />}
              className="size-39.5 bg-widget-tile-2"
            />
            <div className="flex min-w-0 flex-1 flex-col justify-between p-3.5">
              <div className="leading-tight">
                <div className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
                  Latest screenshot
                </div>
                <div className="mt-1 text-notch-w-headline font-semibold">{ago(latest.taken)}</div>
                <div className="mt-0.5 line-clamp-2 text-notch-w-meta break-words text-widget-ink-2">{latest.name}</div>
              </div>
              <div className="flex items-center gap-1.5">
                <Pill tone="primary" onClick={() => onCopy(latest.path)}>
                  Copy
                </Pill>
                <Pill onClick={() => void commands.finderReveal(latest.path)}>Show</Pill>
              </div>
            </div>
          </div>
        </WidgetFrame>
      ) : (
        <InfoTile
          label="Screenshots"
          icon={<CameraIcon strokeWidth={1.6} />}
          title="No screenshots yet"
          action={
            <div>
              <Pill tone="primary" onClick={() => capture("region")}>
                Capture a region
              </Pill>
            </div>
          }
        >
          Press ⇧⌘4, or capture one from here. It shows up as soon as macOS has saved it.
        </InfoTile>
      )}

      {latest && (
        <>
          <WidgetFrame size="small" label="Capture a region" accent="blue" ground="accent" onActivate={() => capture("region")}>
            <div className="flex h-full flex-col justify-between p-3.5">
              <CropIcon aria-hidden className="size-7" strokeWidth={1.6} />
              <span className="text-notch-w-headline leading-tight font-semibold">Region</span>
            </div>
          </WidgetFrame>
          <WidgetFrame size="small" label="To clipboard" ground="vibrant" onActivate={() => capture("clipboard")}>
            <div className="flex h-full flex-col justify-between p-3.5">
              <ClipboardIcon aria-hidden className="size-7 text-widget-ink-2" strokeWidth={1.6} />
              <span className="text-notch-w-headline leading-tight font-semibold">Clipboard</span>
            </div>
          </WidgetFrame>
        </>
      )}

      {rest.map((shot) => (
        <WidgetFrame key={shot.path} size="small" label={formatAgo(shot.taken)} ground="vibrant">
          <div className="relative h-full" {...dragOut(shot.path)}>
            <ShelfThumb path={shot.path} image glyph={<ImageIcon strokeWidth={1.4} />} className="size-full" />
            <div className="absolute right-2 bottom-2 flex gap-1.5">
              <Chip aria-label={`Copy ${shot.name}`} onClick={() => onCopy(shot.path)}>
                <CopyIcon />
              </Chip>
              <Chip aria-label={`Show ${shot.name} in Finder`} onClick={() => void commands.finderReveal(shot.path)}>
                <SearchIcon />
              </Chip>
            </div>
          </div>
        </WidgetFrame>
      ))}

      {watch && (
        <WatchTile
          which="shots"
          on={watch.on}
          onSet={watch.onSet}
          icon={<CameraIcon strokeWidth={1.6} />}
          title="Screenshot alerts"
          subtitle="Drop a chip from the notch when a screenshot is saved"
        />
      )}
    </WidgetBoard>
  );
}
