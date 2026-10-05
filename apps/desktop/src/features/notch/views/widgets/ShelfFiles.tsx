import { useState } from "react";
import { FileIcon, FolderIcon, InboxIcon, MousePointerClickIcon, XIcon } from "lucide-react";
import { Numeral, WidgetBoard, WidgetFrame, formatNumeral } from "@/components/notch/widgets";
import { commands, type ShelfImageFormat, type ShelfItem } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { dragOut, FORMATS } from "../Shelf";
import { useDropToShelf, useNote, useShelfList } from "../shelf-data";
import { Note, Pill, ShelfThumb, InfoTile } from "./shelf-parts";
import { canUnzip, itemMeta, selectedItem } from "./shelf-widgets";

type Result = { status: "ok" } | { status: "error"; error: string };

/** The kind's glyph for a thumbnail that is not a picture (or not made yet). */
const glyphOf = (item: ShelfItem) => (item.kind === "folder" ? <FolderIcon strokeWidth={1.4} /> : <FileIcon strokeWidth={1.4} />);

/**
 * Shelf › Files in the widgets look: Apple's Files widget (a grid of file
 * thumbnails with their names) holding everything on the shelf, and two
 * tiles beside it for what the calm rows carry per file. Click a file to
 * pick it, then AirDrop, convert, unzip or remove it from its tile; drag any
 * file out to another app; drop files from Finder anywhere on the panel.
 * The drop and drag-out are the calm view's own hooks and handlers.
 */
export function WidgetShelfFiles() {
  const [note, setNote] = useNote();
  const dragging = useDropToShelf(setNote);
  const { items } = useShelfList(commands.shelfItems, "files");
  const [selected, setSelected] = useState<string | null>(null);
  const [converting, setConverting] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const list = items ?? [];

  /** Runs one command, showing its failure and nothing on success: the list updates itself. */
  async function run(key: string, job: () => Promise<Result>) {
    setBusy(key);
    setNote(null);
    const r = await job();
    setBusy(null);
    if (r.status === "error") setNote(r.error);
  }
  const ids = list.map((i) => i.id);

  return (
    <>
      <FilesBoard
        items={list}
        dragging={dragging}
        busy={busy}
        selectedId={selected}
        converting={converting}
        onSelect={(id) => {
          setSelected(id);
          setConverting(null);
        }}
        onConverting={setConverting}
        onAirdrop={(which) => void run("airdrop", () => commands.shelfAirdrop(which ?? ids))}
        onZip={() => void run("zip", () => commands.shelfZip(ids))}
        onClear={() => void run("clear", () => commands.shelfClear())}
        onUnzip={(id) => void run(`unzip:${id}`, () => commands.shelfUnzip(id))}
        onRemove={(id) => void run(`remove:${id}`, () => commands.shelfRemove(id))}
        onConvert={(id, format) => {
          setConverting(null);
          void run(`convert:${id}`, () => commands.shelfConvert(id, format));
        }}
      />
      {note && <Note>{note}</Note>}
    </>
  );
}

export function FilesBoard({
  items,
  dragging,
  busy,
  selectedId,
  converting,
  onSelect,
  onConverting,
  onAirdrop,
  onZip,
  onClear,
  onUnzip,
  onRemove,
  onConvert,
}: {
  items: ShelfItem[];
  dragging: boolean;
  busy: string | null;
  selectedId: string | null;
  converting: string | null;
  onSelect: (id: string) => void;
  onConverting: (id: string | null) => void;
  /** No ids means everything on the shelf. */
  onAirdrop: (ids?: string[]) => void;
  onZip: () => void;
  onClear: () => void;
  onUnzip: (id: string) => void;
  onRemove: (id: string) => void;
  onConvert: (id: string, format: ShelfImageFormat) => void;
}) {
  if (items.length === 0) {
    return (
      <WidgetBoard label="Files">
        <InfoTile
          label="Files"
          icon={<InboxIcon strokeWidth={1.6} />}
          title={dragging ? "Release to shelve" : "Nothing on the shelf"}
          ground={dragging ? "vibrant" : "tile"}
        >
          Drop files here and they wait, even after a relaunch. Drag them back out into any app, or AirDrop, zip or
          convert them from here.
        </InfoTile>
      </WidgetBoard>
    );
  }

  const item = selectedItem(items, selectedId);
  const idle = busy !== null;
  return (
    <WidgetBoard label="Files">
      <WidgetFrame size="large" label="Files" ground={dragging ? "tile" : "vibrant"}>
        <ul aria-label="On the shelf" className="m-0 grid h-full list-none grid-cols-3 content-start overflow-y-auto p-0 pt-1.5">
          {items.map((it) => (
            <li key={it.id} className="min-w-0">
              <button
                type="button"
                aria-pressed={it.id === item?.id}
                onClick={() => onSelect(it.id)}
                {...dragOut(it.path)}
                className={cn(
                  "flex h-29.5 w-full cursor-default flex-col items-center rounded-none outline-none",
                  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-solid focus-visible:outline-notch-focus",
                  it.id === item?.id ? "bg-widget-ink/10" : "hover:bg-widget-ink/5",
                )}
              >
                <span className="grid flex-1 place-items-center">
                  <ShelfThumb path={it.path} image={it.image} glyph={glyphOf(it)} className="size-14 rounded-md bg-widget-label text-widget-on" />
                </span>
                {/* The band under each row of thumbnails is how Apple's widget sets the names off. */}
                <span className="w-full truncate bg-widget-ink/5 px-1.5 py-1.5 text-center text-notch-w-over text-widget-label">
                  {it.name}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </WidgetFrame>

      <WidgetFrame size="medium" label="Shelf" ground="tile">
        <div className="flex h-full flex-col justify-between p-3.5">
          <div className="flex items-end gap-2.5">
            <Numeral text={formatNumeral(items.length)} size="l" roll />
            <div className="pb-0.5 leading-tight">
              <div className="text-notch-w-title font-semibold">{dragging ? "Release to shelve" : "on the shelf"}</div>
              <div className="text-notch-w-meta text-widget-ink-2">Drop more here · drag a file out to any app</div>
            </div>
          </div>
          <div className="flex items-center gap-1.5">
            <Pill tone="primary" disabled={idle} onClick={() => onAirdrop()}>
              AirDrop all
            </Pill>
            {items.length > 1 && (
              <Pill disabled={idle} onClick={onZip}>
                {busy === "zip" ? "Zipping…" : "Zip all"}
              </Pill>
            )}
            <Pill tone="danger" disabled={idle} onClick={onClear}>
              Clear
            </Pill>
          </div>
        </div>
      </WidgetFrame>

      {item ? (
        <WidgetFrame size="medium" label="Selected file" ground="tile">
          <div className="flex h-full flex-col justify-between p-3.5">
            <div className="flex items-center gap-3">
              <ShelfThumb path={item.path} image={item.image} glyph={glyphOf(item)} className="size-10 rounded-md bg-widget-ink/10" />
              <div className="min-w-0 leading-tight">
                <div className="truncate text-notch-w-body font-semibold">{item.name}</div>
                <div className="text-notch-w-meta text-widget-ink-2">{itemMeta(item)}</div>
              </div>
            </div>
            {converting === item.id ? (
              <div className="flex items-center gap-1.5">
                {FORMATS.map((f) => (
                  <Pill key={f.id} className="px-2.5" onClick={() => onConvert(item.id, f.id)}>
                    {f.label}
                  </Pill>
                ))}
                <Pill tone="danger" className="px-2.5" onClick={() => onConverting(null)}>
                  Cancel
                </Pill>
              </div>
            ) : (
              <div className="flex items-center gap-1.5">
                <Pill tone="primary" disabled={idle} onClick={() => onAirdrop([item.id])}>
                  AirDrop
                </Pill>
                {item.image && (
                  <Pill disabled={idle} onClick={() => onConverting(item.id)}>
                    {busy === `convert:${item.id}` ? "Converting…" : "Convert"}
                  </Pill>
                )}
                {canUnzip(item) && (
                  <Pill disabled={idle} onClick={() => onUnzip(item.id)}>
                    {busy === `unzip:${item.id}` ? "Unzipping…" : "Unzip"}
                  </Pill>
                )}
                <Pill tone="danger" className="px-2.5" aria-label={`Remove ${item.name}`} onClick={() => onRemove(item.id)}>
                  <XIcon />
                </Pill>
              </div>
            )}
          </div>
        </WidgetFrame>
      ) : (
        <InfoTile label="Selected file" icon={<MousePointerClickIcon strokeWidth={1.6} />} title="Pick a file">
          Click a file to AirDrop, convert, unzip or remove it. Drag it out to hand it to any app.
        </InfoTile>
      )}
    </WidgetBoard>
  );
}
