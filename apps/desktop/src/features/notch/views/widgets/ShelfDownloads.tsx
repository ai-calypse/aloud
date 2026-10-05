import { ArrowDownToLineIcon, DownloadIcon, SearchIcon } from "lucide-react";
import { Numeral, WidgetBoard, WidgetFrame, formatNumeral } from "@/components/notch/widgets";
import { commands, type ShelfDownload } from "@/lib/bindings";
import { dragOut } from "../Shelf";
import { countToday, formatAgo, formatBytes, useDropToShelf, useNote, useShelfList, useShelfPrefs } from "../shelf-data";
import { InfoTile, Note, Pill, ShelfThumb, TurnOnTile, WatchTile } from "./shelf-parts";
import { isImageName } from "./shelf-widgets";

/**
 * Shelf › Downloads in the widgets look: the list as a Developer-style tile
 * (thumbnail, two lines, Show), how many finished today, and the latest one
 * on its own. A download drops from the notch the moment its last byte lands,
 * not when the browser starts it. Rows drag out as real files.
 */
export function WidgetShelfDownloads() {
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
      <WidgetBoard label="Downloads">
        <TurnOnTile
          label="Downloads"
          icon={<DownloadIcon strokeWidth={1.6} />}
          which="downloads"
          onSet={set}
          title="Download alerts are off"
        >
          Turn it on and the notch tells you when a download finishes, in any browser, with a button to show it in
          Finder. macOS will ask once to let Aloud see your Downloads folder.
        </TurnOnTile>
      </WidgetBoard>
    );
  }

  return (
    <>
      <DownloadsBoard downloads={items ?? []} watch={prefs ? { on, onSet: set } : null} />
      {note && <Note>{note}</Note>}
    </>
  );
}

const glyph = <DownloadIcon strokeWidth={1.4} />;

export function DownloadsBoard({
  downloads,
  watch,
  now = Date.now(),
}: {
  downloads: ShelfDownload[];
  watch: { on: boolean; onSet: Parameters<typeof WatchTile>[0]["onSet"] } | null;
  now?: number;
}) {
  const latest = downloads[0];
  return (
    <WidgetBoard label="Downloads">
      {downloads.length === 0 ? (
        <InfoTile label="Downloads" icon={<DownloadIcon strokeWidth={1.6} />} title="No downloads yet">
          Finished downloads from any browser appear here, newest first.
        </InfoTile>
      ) : (
        <>
          <WidgetFrame size="large" label="Recent downloads" ground="vibrant">
            <div className="flex h-full flex-col">
              <h3 className="m-0 px-3.5 pt-3.5 pb-2 text-notch-w-body font-semibold">Recent downloads</h3>
              <ul aria-label="Recent downloads" className="m-0 min-h-0 flex-1 list-none overflow-y-auto p-0 px-3.5">
                {downloads.map((d) => (
                  <li key={d.path} {...dragOut(d.path)} className="flex items-center gap-2.5 border-t border-widget-sep py-2">
                    <ShelfThumb path={d.path} image={isImageName(d.name)} glyph={glyph} className="size-10 rounded-md bg-widget-ink/10" />
                    <span className="min-w-0 flex-1 leading-tight">
                      <span className="block truncate text-notch-w-title font-semibold">{d.name}</span>
                      <span className="block truncate text-notch-w-meta text-widget-ink-2">
                        {formatBytes(d.size)} · {formatAgo(d.finished, now)}
                      </span>
                    </span>
                    <Pill aria-label={`Show ${d.name} in Finder`} onClick={() => void commands.finderReveal(d.path)}>
                      Show
                    </Pill>
                  </li>
                ))}
              </ul>
            </div>
          </WidgetFrame>

          <WidgetFrame size="small" label="Finished today" accent="blue" ground="accent">
            <div className="flex h-full flex-col justify-between p-3.5">
              <ArrowDownToLineIcon aria-hidden className="size-6" strokeWidth={1.6} />
              <div>
                <Numeral text={formatNumeral(countToday(downloads.map((d) => d.finished), now))} size="xl" roll />
                <div className="text-notch-w-title font-semibold">today</div>
                <div className="text-notch-w-meta text-widget-ink/85">Finished in the last 24 hours</div>
              </div>
            </div>
          </WidgetFrame>
        </>
      )}

      {watch && (
        <WatchTile
          which="downloads"
          on={watch.on}
          onSet={watch.onSet}
          icon={<DownloadIcon strokeWidth={1.6} />}
          title="Download alerts"
          subtitle="Drop a chip from the notch when a download finishes"
        />
      )}

      {latest && (
        <WidgetFrame size="medium" label="Latest download">
          <div className="flex h-full items-center gap-3.5 p-3.5" {...dragOut(latest.path)}>
            <ShelfThumb path={latest.path} image={isImageName(latest.name)} glyph={glyph} className="size-22 rounded-notch-row bg-widget-ink/10" />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5 leading-tight">
              <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">Latest</span>
              <span className="line-clamp-2 text-notch-w-body font-semibold break-words">{latest.name}</span>
              <span className="text-notch-w-meta text-widget-ink-2">
                {formatBytes(latest.size)} · {formatAgo(latest.finished, now)}
              </span>
              <Pill tone="primary" className="mt-1.5 w-fit" onClick={() => void commands.finderReveal(latest.path)}>
                <SearchIcon />
                Show
              </Pill>
            </div>
          </div>
        </WidgetFrame>
      )}
    </WidgetBoard>
  );
}
