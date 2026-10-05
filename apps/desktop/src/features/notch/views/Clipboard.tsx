import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  CopyIcon,
  FileIcon,
  ImageIcon,
  PencilIcon,
  PinIcon,
  PipetteIcon,
  TextIcon,
  XIcon,
} from "lucide-react";
import {
  commands,
  type ClipboardEntry,
  type ClipboardKind,
  type PickedColor,
  type Snippet,
} from "@/lib/bindings";
import { ago, filterSnippets, hexToRgb } from "@/lib/clipboard";
import { Button } from "@/components/ui/button";
import { SearchBox, Swatch, TextArea, TextField } from "@/components/notch/field";
import { Switch } from "@/components/notch/controls";
import { Segmented } from "@/components/notch/segmented";
import { Empty, Label, Lead, List, Row, Value } from "@/components/notch/list";
import { AppIcon } from "@/components/notch/app-icon";
import { useLoaded } from "../use-loaded";

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: string };

/** A Rust `Result`'s value, or null with the error said by `say`. */
function ok<T>(res: Res<T>, say: (m: string) => void): T | null {
  if (res.status === "ok") return res.data;
  say(res.error);
  return null;
}

/** Whether a command that returns nothing worked; says why when it did not. */
function done(res: Res<unknown>, say: (m: string) => void): boolean {
  if (res.status === "error") say(res.error);
  return res.status === "ok";
}

/** A line of feedback that clears itself, shown where the list's label is. */
function useFlash(): [string | null, (message: string) => void] {
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return [
    message,
    (next) => {
      setMessage(next);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setMessage(null), 1800);
    },
  ];
}

const KIND_ICON = { text: TextIcon, image: ImageIcon, files: FileIcon } satisfies Record<ClipboardKind, unknown>;
const KIND_WORD: Record<ClipboardKind, string> = { text: "Text", image: "Image", files: "Files" };

/** The 26px lead tile: an image's own thumbnail, else a glyph for its kind. */
function Lead26({ entry }: { entry: ClipboardEntry }) {
  if (entry.thumbnail) return <AppIcon icon={entry.thumbnail} label={entry.preview} />;
  const Glyph = KIND_ICON[entry.kind];
  return (
    <span
      aria-hidden
      className="inline-flex size-[26px] shrink-0 items-center justify-center rounded-notch-control bg-notch-well text-notch-ink-2"
    >
      <Glyph className="size-3.5" strokeWidth={1.8} />
    </span>
  );
}

function IconButton({
  label,
  onClick,
  pressed,
  children,
}: {
  label: string;
  onClick: () => void;
  pressed?: boolean;
  children: ReactNode;
}) {
  return (
    <Button
      variant="notch-quiet"
      size="notch-icon"
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

/**
 * The on/off row. History defaults off and says exactly what turning it on
 * means, because a clipboard is where passwords and private messages pass.
 */
function HistorySwitch({ on, onChange }: { on: boolean; onChange: (on: boolean) => void }) {
  return (
    <List label="Clipboard history setting">
      <Row
        title="Keep clipboard history"
        subtitle="This Mac only. Never logged, never sent to a model."
        trailing={<Switch checked={on} onChange={onChange} label="Keep clipboard history" />}
      />
    </List>
  );
}

/** Clipboard › History: what was copied, to copy again, paste, pin or act on. */
export function ClipboardHistory() {
  const [query, setQuery] = useState("");
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [flash, say] = useFlash();
  const { data: entries, reload } = useLoaded(
    async () => ok(await commands.clipboardList(query), say),
    [query],
    true,
  );

  useEffect(() => {
    void commands.clipboardEnabled().then(setEnabled);
  }, []);

  const toggle = async (on: boolean) => {
    if (done(await commands.clipboardSetEnabled(on), say)) {
      setEnabled(on);
      reload();
    }
  };
  const run = async (promise: Promise<Res<unknown>>, message?: string) => {
    if (done(await promise, say) && message) say(message);
    reload();
  };

  if (enabled === null) return null;
  if (!enabled) {
    return (
      <>
        <Empty title="Clipboard history is off">
          Turn it on and Aloud keeps what you copy — text, images and files — so you can copy it again,
          paste it, or act on it. It all stays on this Mac: it is never logged, never sent anywhere or
          shown to a model, and copies from password managers are skipped.
        </Empty>
        <div className="mt-3">
          <HistorySwitch on={false} onChange={(on) => void toggle(on)} />
        </div>
      </>
    );
  }

  const list = entries ?? [];
  return (
    <>
      <SearchBox
        label="Search clipboard"
        value={query}
        onChange={setQuery}
        trailing={
          <Button
            variant="notch-danger"
            size="notch"
            disabled={list.every((e) => e.pinned)}
            onClick={() => void run(commands.clipboardClear(), "Cleared")}
          >
            Clear
          </Button>
        }
      />
      <Label className="mt-2">{flash ?? (query ? "Matches" : "Recent copies")}</Label>
      {list.length === 0 ? (
        <Empty title={query ? "Nothing matches" : "Nothing copied yet"}>
          {query ? "Try fewer words." : "Copy something and it shows up here."}
        </Empty>
      ) : (
        <List label="Clipboard history">
          {list.map((entry) => {
            // One smart action leads; the row has room for no more.
            const smart = entry.actions[0];
            return (
              <Row
                key={entry.id}
                lead={<Lead26 entry={entry} />}
                title={entry.preview.replace(/\s+/g, " ")}
                subtitle={`${ago(entry.copiedAt)} · ${KIND_WORD[entry.kind]}`}
                trailing={
                  <span className="flex shrink-0 items-center gap-0.5">
                    {smart && (
                      <Button
                        variant="notch-quiet"
                        size="notch"
                        onClick={() => void run(commands.clipboardAction(entry.id, smart.id), "Done")}
                      >
                        {smart.label}
                      </Button>
                    )}
                    <Button
                      variant="notch-quiet"
                      size="notch"
                      onClick={() => void run(commands.clipboardPaste(entry.id))}
                    >
                      Paste
                    </Button>
                    <IconButton label="Copy" onClick={() => void run(commands.clipboardCopy(entry.id), "Copied")}>
                      <CopyIcon strokeWidth={1.8} />
                    </IconButton>
                    <IconButton
                      label={entry.pinned ? "Unpin" : "Pin"}
                      pressed={entry.pinned}
                      onClick={() => void run(commands.clipboardPin(entry.id, !entry.pinned))}
                    >
                      <PinIcon strokeWidth={1.8} className={entry.pinned ? "fill-current" : undefined} />
                    </IconButton>
                    <IconButton label="Delete" onClick={() => void run(commands.clipboardDelete(entry.id))}>
                      <XIcon strokeWidth={1.8} />
                    </IconButton>
                  </span>
                }
              />
            );
          })}
        </List>
      )}
      <div className="mt-3">
        <HistorySwitch on onChange={(on) => void toggle(on)} />
      </div>
    </>
  );
}

/** Clipboard › Snippets: text kept to paste again, with a name and a short trigger word. */
export function ClipboardSnippets() {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Partial<Snippet> | null>(null);
  const [flash, say] = useFlash();
  const { data, reload } = useLoaded(async () => ok(await commands.snippetList(), say), []);
  const list = filterSnippets(data ?? [], query);

  if (editing) {
    return (
      <SnippetForm
        snippet={editing}
        onCancel={() => setEditing(null)}
        onSaved={() => {
          setEditing(null);
          reload();
        }}
      />
    );
  }
  if (data && data.length === 0) {
    return (
      <Empty
        title="No snippets yet"
        action={
          <Button variant="notch" size="notch" onClick={() => setEditing({})}>
            New snippet
          </Button>
        }
      >
        Keep the text you type again and again — a signature, an address, a reply — and paste it from here.
      </Empty>
    );
  }
  return (
    <>
      <SearchBox
        label="Search snippets"
        value={query}
        onChange={setQuery}
        trailing={
          <Button variant="notch" size="notch" onClick={() => setEditing({})}>
            New snippet
          </Button>
        }
      />
      <Label className="mt-2">{flash ?? "Snippets"}</Label>
      <List label="Snippets">
        {list.map((snippet) => (
          <Row
            key={snippet.id}
            title={snippet.name}
            subtitle={[snippet.trigger, snippet.body.replace(/\s+/g, " ")].filter(Boolean).join(" · ")}
            trailing={
              <span className="flex shrink-0 items-center gap-0.5">
                <Button
                  variant="notch-quiet"
                  size="notch"
                  onClick={() => void commands.snippetPaste(snippet.id)}
                >
                  Paste
                </Button>
                <IconButton
                  label="Copy"
                  onClick={() =>
                    void commands.snippetCopy(snippet.id).then((r) => done(r, say) && say("Copied"))
                  }
                >
                  <CopyIcon strokeWidth={1.8} />
                </IconButton>
                <IconButton label="Edit" onClick={() => setEditing(snippet)}>
                  <PencilIcon strokeWidth={1.8} />
                </IconButton>
                <IconButton
                  label="Delete"
                  onClick={() => void commands.snippetDelete(snippet.id).then(() => reload())}
                >
                  <XIcon strokeWidth={1.8} />
                </IconButton>
              </span>
            }
          />
        ))}
      </List>
    </>
  );
}

function SnippetForm({
  snippet,
  onCancel,
  onSaved,
}: {
  snippet: Partial<Snippet>;
  onCancel: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState(snippet.name ?? "");
  const [trigger, setTrigger] = useState(snippet.trigger ?? "");
  const [body, setBody] = useState(snippet.body ?? "");
  const [error, setError] = useState<string | null>(null);
  const save = async () => {
    const res = await commands.snippetSave(snippet.id ?? null, name, trigger, body);
    if (res.status === "ok") onSaved();
    else setError(res.error);
  };
  return (
    <form
      className="flex flex-col gap-2 font-notch"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <div className="grid grid-cols-[2fr_1fr] gap-2">
        <TextField autoFocus aria-label="Name" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
        <TextField
          aria-label="Trigger"
          placeholder="Trigger (optional)"
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
        />
      </div>
      <TextArea aria-label="Text" placeholder="Text to paste" value={body} onChange={(e) => setBody(e.target.value)} />
      {error && <p className="m-0 text-notch-meta text-notch-danger">{error}</p>}
      <div className="flex items-center gap-1">
        <Button type="submit" variant="notch" size="notch">
          Save
        </Button>
        <Button type="button" variant="notch-quiet" size="notch" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

/** Clipboard › Colors: the screen eyedropper and the colours it has picked. */
export function ClipboardColors() {
  const [format, setFormat] = useState<"hex" | "rgb">("hex");
  const [picking, setPicking] = useState(false);
  const [flash, say] = useFlash();
  const { data, reload } = useLoaded(async () => ok(await commands.colorList(), say), []);

  const pick = async () => {
    setPicking(true);
    const res = await commands.colorPick();
    setPicking(false);
    if (ok(res, say)) say("Copied"); // null is a cancelled pick
    reload();
  };
  const copy = (color: PickedColor) =>
    void commands.colorCopy(color.hex, format).then((r) => done(r, say) && say("Copied"));

  const picker = (
    <Button variant="notch" size="notch" disabled={picking} onClick={() => void pick()}>
      <PipetteIcon strokeWidth={1.8} />
      {picking ? "Picking…" : "Pick color"}
    </Button>
  );
  const colors = data ?? [];
  if (data && colors.length === 0) {
    return (
      <Empty title="No colors picked yet" action={picker}>
        Pick any color on screen. Its hex is copied, and it stays here to copy again as hex or RGB.
      </Empty>
    );
  }
  const [latest, ...earlier] = colors;
  const show = (c: PickedColor) => (format === "hex" ? c.hex : hexToRgb(c.hex));
  return (
    <>
      {latest && (
        <div className="flex items-center gap-3 font-notch">
          <Swatch color={latest.hex} size="lg" />
          <Lead sub={format === "hex" ? hexToRgb(latest.hex) : latest.hex}>{show(latest)}</Lead>
          <div className="ml-auto flex items-center gap-2">
            <Segmented
              label="Copy as"
              value={format}
              onChange={(id) => setFormat(id as "hex" | "rgb")}
              items={[
                { id: "hex", label: "Hex" },
                { id: "rgb", label: "RGB" },
              ]}
            />
            <Button variant="notch-quiet" size="notch" onClick={() => copy(latest)}>
              Copy
            </Button>
            {picker}
          </div>
        </div>
      )}
      {earlier.length > 0 && (
        <>
          <Label className="mt-3">{flash ?? "Earlier"}</Label>
          <List label="Picked colors">
            {earlier.map((color) => (
              <Row
                key={color.id}
                lead={<Swatch color={color.hex} />}
                title={show(color)}
                subtitle={ago(color.pickedAt)}
                trailing={<Value>{format === "hex" ? "Copy hex" : "Copy RGB"}</Value>}
                onClick={() => copy(color)}
              />
            ))}
          </List>
        </>
      )}
      {earlier.length === 0 && flash && <Label className="mt-3">{flash}</Label>}
    </>
  );
}
