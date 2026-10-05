import { useState, type ReactNode } from "react";
import { XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AppIcon } from "@/components/notch/app-icon";
import { Switch } from "@/components/notch/controls";
import { Empty, Lead, List, Row, Value } from "@/components/notch/list";
import {
  commands,
  type ScreenShot,
  type ShelfDownload,
  type ShelfDrive,
  type ShelfImageFormat,
  type ShelfItem,
  type ShelfWatch,
} from "@/lib/bindings";
import { cn } from "@/lib/utils";
import {
  countToday,
  formatAgo,
  formatBytes,
  useDropToShelf,
  useNote,
  useShelfList,
  useShelfPrefs,
  useThumb,
} from "./shelf-data";

/** The lead's phrasing of `formatAgo`: "Just now", "2m ago". */
export function ago(secs: number): string {
  const t = formatAgo(secs);
  return t === "now" ? "Just now" : `${t} ago`;
}

/**
 * What arms a drag out of a row: Rust starts the system drag on the next
 * mouse move (`crates/drag`), so a row with a real file behind it can be
 * dropped into Slack, Mail or Finder. The mouseup disarms it, so a plain
 * click never turns into a drag later.
 */
export function dragOut(path: string) {
  return {
    onMouseDown: () => void commands.armFileDrag(path),
    onMouseUp: () => void commands.disarmFileDrag(),
  };
}

/** A picture's thumbnail, or its kind's glyph until (or instead of) one. */
function Thumb({ path, image, glyph }: { path: string; image: boolean; glyph: string }) {
  const src = useThumb(path, image);
  return src ? (
    <img
      src={src}
      alt=""
      draggable={false}
      className="size-[26px] shrink-0 rounded-notch-control object-cover"
    />
  ) : (
    <AppIcon icon={glyph} label="" size="md" />
  );
}

/** What went wrong, or what just happened, under a list. */
function Note({ children }: { children: ReactNode }) {
  return (
    <p role="status" className="m-0 mt-2 font-notch text-notch-meta text-notch-ink-2">
      {children}
    </p>
  );
}

/**
 * The view's on/off row: whether Aloud is watching this folder at all. A
 * watcher that cannot start (macOS has not allowed the folder yet) says why
 * and stays off.
 */
function WatchRow({
  which,
  on,
  title,
  subtitle,
  onSet,
}: {
  which: ShelfWatch;
  on: boolean;
  title: string;
  subtitle: string;
  onSet: (which: ShelfWatch, on: boolean) => Promise<string | null>;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <List label="Setting" className="mt-3">
        <Row
          title={title}
          subtitle={subtitle}
          trailing={
            <Switch
              label={title}
              checked={on}
              onChange={(next) => void onSet(which, next).then(setError)}
            />
          }
        />
      </List>
      {error && <Note>{error}</Note>}
    </>
  );
}

/** The view for a watcher that is off: what it would do, and the one button. */
function TurnOn({
  title,
  children,
  which,
  onSet,
}: {
  title: string;
  children: ReactNode;
  which: ShelfWatch;
  onSet: (which: ShelfWatch, on: boolean) => Promise<string | null>;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <Empty
        title={title}
        action={
          <Button variant="notch" size="notch" onClick={() => void onSet(which, true).then(setError)}>
            Turn on
          </Button>
        }
      >
        {children}
      </Empty>
      {error && <Note>{error}</Note>}
    </>
  );
}

export const FORMATS: { id: ShelfImageFormat; label: string }[] = [
  { id: "png", label: "PNG" },
  { id: "jpeg", label: "JPEG" },
  { id: "heic", label: "HEIC" },
  { id: "tiff", label: "TIFF" },
];

/**
 * Shelf › Files: things parked on the notch. Drop files from Finder onto the
 * open panel, drag a row out into any app, AirDrop it, zip it, or turn a
 * picture into another format. Each item is a copy, so it is still here
 * after the original moves and after a relaunch.
 */
export function ShelfFiles() {
  const [note, setNote] = useNote();
  const dragging = useDropToShelf(setNote);
  const { items } = useShelfList(commands.shelfItems, "files");
  const [converting, setConverting] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const list = items ?? [];

  /** Runs one command, showing its failure and nothing on success: the list updates itself. */
  async function run(key: string, job: () => Promise<{ status: "ok" } | { status: "error"; error: string }>) {
    setBusy(key);
    setNote(null);
    const r = await job();
    setBusy(null);
    if (r.status === "error") setNote(r.error);
  }

  const airdrop = (ids: string[]) => run("airdrop", () => commands.shelfAirdrop(ids));
  const ids = list.map((i) => i.id);
  const isZip = (item: ShelfItem) => item.kind === "file" && /\.zip$/i.test(item.name);

  return (
    <div
      className={cn(
        "-m-1.5 rounded-notch-row p-1.5 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
        dragging && "bg-notch-well",
      )}
    >
      {list.length === 0 ? (
        <Empty title={dragging ? "Release to shelve" : "Nothing on the shelf"}>
          Drop files here and they wait, even after a relaunch. Drag them back
          out into any app, or AirDrop, zip or convert them from here.
        </Empty>
      ) : (
        <>
          <div className="flex items-end justify-between gap-3">
            <Lead sub={dragging ? "Release to shelve" : "Drop more here · drag a row out to any app"}>
              {list.length} on the shelf
            </Lead>
            <div className="flex shrink-0 items-center gap-1">
              <Button
                variant="notch"
                size="notch"
                disabled={busy !== null}
                onClick={() => void airdrop(ids)}
              >
                AirDrop all
              </Button>
              {list.length > 1 && (
                <Button
                  variant="notch-quiet"
                  size="notch"
                  disabled={busy !== null}
                  onClick={() => void run("zip", () => commands.shelfZip(ids))}
                >
                  {busy === "zip" ? "Zipping…" : "Zip all"}
                </Button>
              )}
              <Button
                variant="notch-danger"
                size="notch"
                disabled={busy !== null}
                onClick={() => void run("clear", () => commands.shelfClear())}
              >
                Clear
              </Button>
            </div>
          </div>
          <List label="On the shelf" className="mt-3">
            {list.map((item) => (
              <Row
                key={item.id}
                pointer={dragOut(item.path)}
                lead={
                  <Thumb path={item.path} image={item.image} glyph={item.kind === "folder" ? "folder" : "file"} />
                }
                title={item.name}
                subtitle={item.kind === "folder" ? "Folder" : formatBytes(item.size)}
                trailing={
                  converting === item.id ? (
                    <span className="flex items-center gap-0.5">
                      {FORMATS.map((f) => (
                        <Button
                          key={f.id}
                          variant="notch-quiet"
                          size="notch"
                          onClick={() => {
                            setConverting(null);
                            void run(`convert:${item.id}`, () => commands.shelfConvert(item.id, f.id));
                          }}
                        >
                          {f.label}
                        </Button>
                      ))}
                      <Button variant="notch-quiet" size="notch" onClick={() => setConverting(null)}>
                        Cancel
                      </Button>
                    </span>
                  ) : (
                    <span className="flex items-center gap-0.5">
                      {item.image && (
                        <Button
                          variant="notch-quiet"
                          size="notch"
                          disabled={busy !== null}
                          onClick={() => setConverting(item.id)}
                        >
                          {busy === `convert:${item.id}` ? "Converting…" : "Convert"}
                        </Button>
                      )}
                      {isZip(item) && (
                        <Button
                          variant="notch-quiet"
                          size="notch"
                          disabled={busy !== null}
                          onClick={() => void run(`unzip:${item.id}`, () => commands.shelfUnzip(item.id))}
                        >
                          {busy === `unzip:${item.id}` ? "Unzipping…" : "Unzip"}
                        </Button>
                      )}
                      <Button
                        variant="notch-quiet"
                        size="notch"
                        disabled={busy !== null}
                        onClick={() => void airdrop([item.id])}
                      >
                        AirDrop
                      </Button>
                      <Button
                        variant="notch-quiet"
                        size="notch-icon"
                        aria-label={`Remove ${item.name}`}
                        onClick={() => void run(`remove:${item.id}`, () => commands.shelfRemove(item.id))}
                      >
                        <XIcon />
                      </Button>
                    </span>
                  )
                }
              />
            ))}
          </List>
        </>
      )}
      {note && <Note>{note}</Note>}
    </div>
  );
}

/**
 * Shelf › Screenshots: what macOS saved lately, from whichever folder the
 * Screenshot app is set to. A new one drops from the notch with Copy and
 * Show; every row drags out as a real file.
 */
export function ShelfShots() {
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
      <TurnOn which="shots" onSet={set} title="Screenshot shelf is off">
        Turn it on and each new screenshot drops from the notch, ready to copy,
        show in Finder or drag into another app. macOS will ask once to let Aloud
        see the folder your screenshots go to.
      </TurnOn>
    );
  }
  const shots = items ?? [];

  async function copy(path: string) {
    const r = await commands.shotsCopy(path);
    setNote(r.status === "ok" ? "Copied" : r.error);
  }

  return (
    <>
      {shots.length === 0 ? (
        <Empty
          title="No screenshots yet"
          action={
            <Button variant="notch" size="notch" onClick={() => void commands.shotsCapture("region")}>
              Capture a region
            </Button>
          }
        >
          Press ⇧⌘4, or capture one from here. It shows up as soon as macOS has
          saved it.
        </Empty>
      ) : (
        <>
          <div className="flex items-end justify-between gap-3">
            <Lead sub="Latest screenshot">{ago(shots[0].taken)}</Lead>
            <div className="flex shrink-0 items-center gap-1">
              <Button variant="notch" size="notch" onClick={() => void commands.shotsCapture("region")}>
                Capture a region
              </Button>
              <Button variant="notch-quiet" size="notch" onClick={() => void commands.shotsCapture("clipboard")}>
                To clipboard
              </Button>
            </div>
          </div>
          <List label="Recent screenshots" className="mt-3">
            {shots.map((shot) => (
              <Row
                key={shot.path}
                pointer={dragOut(shot.path)}
                lead={<Thumb path={shot.path} image glyph="image" />}
                title={shot.name}
                subtitle={formatAgo(shot.taken)}
                trailing={
                  <span className="flex items-center gap-0.5">
                    <Button variant="notch-quiet" size="notch" onClick={() => void copy(shot.path)}>
                      Copy
                    </Button>
                    <Button
                      variant="notch-quiet"
                      size="notch"
                      onClick={() => void commands.finderReveal(shot.path)}
                    >
                      Show
                    </Button>
                  </span>
                }
              />
            ))}
          </List>
        </>
      )}
      {note && <Note>{note}</Note>}
      {prefs && (
        <WatchRow
          which="shots"
          on={on}
          onSet={set}
          title="Screenshot alerts"
          subtitle="Drop a chip from the notch when a screenshot is saved"
        />
      )}
    </>
  );
}

/**
 * Shelf › Downloads: what finished lately, from any browser. A download
 * drops from the notch the moment its last byte lands — not when the
 * browser starts it, and not for a half-written `.crdownload`.
 */
export function ShelfDownloads() {
  const { prefs, set } = useShelfPrefs();
  const [note, setNote] = useNote();
  useDropToShelf(setNote);
  const on = prefs?.downloads ?? false;
  const { items } = useShelfList<ShelfDownload>(
    async () => {
      const r = await commands.downloadsRecent(12);
      if (r.status === "error") throw new Error(r.error);
      return r.data;
    },
    "downloads",
    on,
  );

  if (prefs && !on) {
    return (
      <TurnOn which="downloads" onSet={set} title="Download alerts are off">
        Turn it on and the notch tells you when a download finishes, in any
        browser, with a button to show it in Finder. macOS will ask once to let
        Aloud see your Downloads folder.
      </TurnOn>
    );
  }
  const downloads = items ?? [];

  return (
    <>
      {downloads.length === 0 ? (
        <Empty title="No downloads yet">
          Finished downloads from any browser appear here, newest first.
        </Empty>
      ) : (
        <>
          <Lead sub="Finished in the last 24 hours">
            {countToday(downloads.map((d) => d.finished))} today
          </Lead>
          <List label="Recent downloads" className="mt-3">
            {downloads.map((d) => (
              <Row
                key={d.path}
                pointer={dragOut(d.path)}
                lead={<Thumb path={d.path} image={/\.(png|jpe?g|gif|heic|webp|tiff?)$/i.test(d.name)} glyph="download" />}
                title={d.name}
                subtitle={`${formatBytes(d.size)} · ${formatAgo(d.finished)}`}
                trailing={
                  <Button variant="notch-quiet" size="notch" onClick={() => void commands.finderReveal(d.path)}>
                    Show
                  </Button>
                }
              />
            ))}
          </List>
        </>
      )}
      {note && <Note>{note}</Note>}
      {prefs && (
        <WatchRow
          which="downloads"
          on={on}
          onSet={set}
          title="Download alerts"
          subtitle="Drop a chip from the notch when a download finishes"
        />
      )}
    </>
  );
}

/**
 * Shelf › Drives: external drives and mounted disk images, each with Eject.
 * A drive that is in use says what is holding it rather than failing quietly.
 */
export function ShelfDrives() {
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
  const drives = items ?? [];

  async function eject(drive: ShelfDrive) {
    setEjecting(drive.mount);
    setNote(null);
    const r = await commands.driveEject(drive.mount);
    setEjecting(null);
    setNote(r.status === "ok" ? `${drive.name} is safe to disconnect` : r.error);
  }

  return (
    <>
      {drives.length === 0 ? (
        <Empty title="No external drives">
          Plug in a USB drive or SD card, or mount a disk image, and it shows up
          here with an Eject button.
        </Empty>
      ) : (
        <>
          <Lead sub={drives.length === 1 ? "drive connected" : "drives connected"}>{drives.length}</Lead>
          <List label="Drives" className="mt-3">
            {drives.map((drive) => (
              <Row
                key={drive.mount}
                lead={<AppIcon icon="drive" label="" size="md" />}
                title={drive.name}
                subtitle={[drive.bus, `${formatBytes(drive.free)} free of ${formatBytes(drive.size)}`]
                  .filter(Boolean)
                  .join(" · ")}
                trailing={
                  <>
                    <Value>{drive.size > 0 ? `${Math.round(((drive.size - drive.free) / drive.size) * 100)}% full` : ""}</Value>
                    <Button
                      // The one primary only when there is one thing to eject.
                      variant={drives.length === 1 ? "notch" : "notch-quiet"}
                      size="notch"
                      disabled={ejecting !== null}
                      onClick={() => void eject(drive)}
                    >
                      {ejecting === drive.mount ? "Ejecting…" : "Eject"}
                    </Button>
                  </>
                }
              />
            ))}
          </List>
        </>
      )}
      {note && <Note>{note}</Note>}
      {prefs && (
        <WatchRow
          which="drives"
          on={prefs.drives !== false}
          onSet={set}
          title="Drive alerts"
          subtitle="Drop a chip from the notch when a drive is connected"
        />
      )}
    </>
  );
}

