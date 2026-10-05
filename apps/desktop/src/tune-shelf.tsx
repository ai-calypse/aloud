import type { ReactNode } from "react";
import type { ShelfDownload, ShelfDrive } from "./lib/bindings";
import { thumbs } from "./features/notch/views/shelf-data";
import { DownloadsBoard } from "./features/notch/views/widgets/ShelfDownloads";
import { DrivesBoard } from "./features/notch/views/widgets/ShelfDrives";

/**
 * Shelf board fixtures (Downloads, Drives) for the tune page. Everything here is a
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

export function ShelfGallery() {
  return (
    <div className="grid max-w-[720px] gap-9 font-notch-w">
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
