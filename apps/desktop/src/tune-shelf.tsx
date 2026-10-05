import { useState, type ReactNode } from "react";
import type { ScreenShot, ShelfDownload, ShelfDrive, ShelfItem } from "./lib/bindings";
import { thumbs } from "./features/notch/views/shelf-data";
import { DownloadsBoard } from "./features/notch/views/widgets/ShelfDownloads";
import { DrivesBoard } from "./features/notch/views/widgets/ShelfDrives";
import { FilesBoard } from "./features/notch/views/widgets/ShelfFiles";
import { ShotsBoard } from "./features/notch/views/widgets/ShelfShots";
import { TurnOnTile } from "./features/notch/views/widgets/shelf-parts";
import { WidgetBoard } from "./components/notch/widgets";
import { CameraIcon } from "lucide-react";

/**
 * Widgets look › Shelf fixtures for the tune page. Everything here is a
 * labelled fixture: the app never shows it. The thumbnails are generated
 * gradients seeded into the real thumbnail cache (`shelf-data`), so the tiles
 * run their real `useThumb` path with no Tauri behind it.
 */

const NOW = Date.now();
const SEC = Math.floor(NOW / 1000);

/** A stand-in picture: a gradient with a window-ish block, 64px like Rust's thumbnails. */
function art(hue: number, kind: "page" | "shot" = "shot"): string {
  const a = `hsl(${hue} 70% 55%)`;
  const b = `hsl(${(hue + 50) % 360} 70% 40%)`;
  const body =
    kind === "page"
      ? `<rect width="64" height="64" fill="#f2f2f2"/><g fill="#bbb"><rect x="10" y="14" width="44" height="3"/><rect x="10" y="22" width="44" height="3"/><rect x="10" y="30" width="44" height="3"/><rect x="10" y="38" width="30" height="3"/></g>`
      : `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="64" height="64" fill="url(#g)"/><rect x="8" y="12" width="48" height="40" rx="4" fill="#fff" fill-opacity=".85"/><rect x="8" y="12" width="48" height="8" rx="4" fill="#000" fill-opacity=".25"/>`;
  return `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">${body}</svg>`)}`;
}

const seed = (path: string, hue: number, kind?: "page" | "shot") => {
  thumbs.set(path, Promise.resolve(art(hue, kind)));
  return path;
};

const FILE_NAMES: [string, boolean, "file" | "folder"][] = [
  ["report-q3.pdf", false, "file"],
  ["demo.mov", false, "file"],
  ["icons.zip", false, "file"],
  ["mock.png", true, "file"],
  ["notes.md", false, "file"],
  ["Screen 4.png", true, "file"],
  ["invoice.pdf", false, "file"],
  ["Assets", false, "folder"],
  ["deck.key", false, "file"],
  ["photo.heic", true, "file"],
  ["contract.pdf", false, "file"],
];

const ITEMS: ShelfItem[] = FILE_NAMES.map(([name, image, kind], i) => ({
  id: `f${i}`,
  name,
  path: image ? seed(`/fixture/files/${name}`, i * 47) : `/fixture/files/${name}`,
  original: `/fixture/${name}`,
  kind,
  size: 400_000 + i * 730_000,
  added: SEC - i * 600,
  image,
}));

const SHOTS: ScreenShot[] = Array.from({ length: 7 }, (_, i) => ({
  name: `Screenshot 2026-10-04 at 10.${40 - i}.12.png`,
  path: seed(`/fixture/shots/${i}`, 200 + i * 30),
  taken: SEC - 30 - i * 1900,
}));

const DOWNLOADS: ShelfDownload[] = [
  ["Aloud-0.9.2.dmg", 84_000_000, 120],
  ["wireframes.png", 1_900_000, 3_100],
  ["Q3 board deck.pdf", 6_400_000, 9_000],
  ["dataset.csv", 31_000_000, 40_000],
  ["Invoice 4411.pdf", 180_000, 90_000],
  ["photo-from-maya.heic", 2_800_000, 200_000],
].map(([name, size, ago], i) => ({
  name: name as string,
  path: /\.(png|heic)$/.test(name as string) ? seed(`/fixture/dl/${i}`, 20 + i * 40) : `/fixture/dl/${name}`,
  size: size as number,
  finished: SEC - (ago as number),
}));

const DRIVES: ShelfDrive[] = [
  { name: "SANDISK 64", mount: "/Volumes/SANDISK 64", bus: "USB", size: 64_000_000_000, free: 41_000_000_000 },
  { name: "Photos SD", mount: "/Volumes/Photos SD", bus: "Secure Digital", size: 128_000_000_000, free: 21_000_000_000 },
  { name: "Backup", mount: "/Volumes/Backup", bus: "USB", size: 1_000_000_000_000, free: 22_000_000_000 },
];

const WATCH = { on: true, onSet: async () => null as string | null };
const none = () => {};

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

/** Selection and the format picker are live here; the commands are not wired. */
function FilesFixture({ items, dragging = false, first }: { items: ShelfItem[]; dragging?: boolean; first?: string }) {
  const [selected, setSelected] = useState<string | null>(first ?? null);
  const [converting, setConverting] = useState<string | null>(null);
  return (
    <FilesBoard
      items={items}
      dragging={dragging}
      busy={null}
      selectedId={selected}
      converting={converting}
      onSelect={(id) => {
        setSelected(id);
        setConverting(null);
      }}
      onConverting={setConverting}
      onAirdrop={none}
      onZip={none}
      onClear={none}
      onUnzip={none}
      onRemove={none}
      onConvert={none}
    />
  );
}

export function ShelfGallery() {
  return (
    <div data-notch-style="widgets" className="grid max-w-[720px] gap-9 font-notch-w">
      <Block name="Shelf · Files" note="fixture: 11 files (scrolls past nine), a picture selected; click another, then Convert for the formats">
        <FilesFixture items={ITEMS} first="f3" />
      </Block>
      <Block name="Shelf · Files, a zip and a folder" note="fixture: Unzip appears for a .zip only; a folder has no Convert and says Folder">
        <FilesFixture items={ITEMS.slice(0, 9)} first="f2" />
      </Block>
      <Block name="Shelf · Files, empty and over the drop" note="the calm view's words; the panel is the drop target, so it says release">
        <div className="grid gap-6">
          <FilesFixture items={[]} />
          <FilesFixture items={[]} dragging />
        </div>
      </Block>
      <Block name="Shelf · Screenshots" note="fixture: seven, newest leading; Copy, Show, drag out; capture tiles; the alerts switch">
        <ShotsBoard shots={SHOTS} onCopy={none} watch={WATCH} />
      </Block>
      <Block name="Shelf · Screenshots, empty and off" note="empty with the one action; then the watcher off, with macOS's permission note">
        <div className="grid gap-6">
          <ShotsBoard shots={[]} onCopy={none} watch={WATCH} />
          <WidgetBoard label="Screenshots off">
            <TurnOnTile
              label="Screenshots"
              icon={<CameraIcon strokeWidth={1.6} />}
              which="shots"
              onSet={async () => "macOS has not let Aloud see that folder yet. Allow it in System Settings, then try again."}
              title="Screenshot shelf is off"
            >
              Turn it on and each new screenshot drops from the notch, ready to copy, show in Finder or drag into
              another app. macOS will ask once to let Aloud see the folder your screenshots go to.
            </TurnOnTile>
          </WidgetBoard>
        </div>
      </Block>
      <Block name="Shelf · Downloads" note="fixture: six downloads, the list, today's count, the latest">
        <DownloadsBoard downloads={DOWNLOADS} watch={WATCH} now={NOW} />
      </Block>
      <Block name="Shelf · Downloads, empty" note="nothing finished yet">
        <DownloadsBoard downloads={[]} watch={WATCH} />
      </Block>
      <Block name="Shelf · Drives" note="fixture: healthy, getting full (amber), almost full (red); every ring has its percentage and word">
        <DrivesBoard drives={DRIVES} ejecting={null} onEject={none} watch={WATCH} />
      </Block>
      <Block name="Shelf · Drives, one and none" note="a lone drive gets the primary Eject; none teaches what fills it">
        <div className="grid gap-6">
          <DrivesBoard drives={DRIVES.slice(0, 1)} ejecting={null} onEject={none} watch={{ ...WATCH, on: false }} />
          <DrivesBoard drives={[]} ejecting={null} onEject={none} watch={WATCH} />
        </div>
      </Block>
    </div>
  );
}
