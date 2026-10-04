import type { ReactNode } from "react";
import { ArrowRightIcon } from "lucide-react";
import type { ActionPreview, CardKind, Field, Role } from "@/lib/bindings";
import {
  AppIcon,
  Bubble,
  DiffRow,
  FieldRows,
  FileGlyph,
  HeadingText,
  ItemSample,
  MessageBody,
  MoneyLine,
  MoreFields,
  OriginTag,
  RiskLine,
  SaferLine,
  SheetTable,
  Tags,
  TextBlock,
  UnresolvedId,
  Value,
  WhenBlock,
} from "./kit";

/**
 * One small window per kind of action, drawn the way its app draws it: a
 * compose window for an email, a bubble for a chat, an event's time large.
 * Rust picks the kind and fills every field; a layout only decides where each
 * one sits. It never formats a value and never drops a field — whatever a
 * layout does not place itself goes to `Flow` at its foot.
 */

type Body = (props: { preview: ActionPreview }) => ReactNode;

type Layout = {
  /** The window's title, as its app would put it, or the card's own. */
  title: (preview: ActionPreview, app: string) => ReactNode;
  body: Body;
};

/** The fields with one of `roles`, and the rest, each in their own order. */
function split(fields: Field[], ...roles: Role[]): [Field[], Field[]] {
  return [
    fields.filter((f) => roles.includes(f.role)),
    fields.filter((f) => !roles.includes(f.role)),
  ];
}

/** Text, and rows of sheet cells: too wide for a row's value column. */
const wide = (field: Field) => field.role === "text" || field.cells.length > 0;

/**
 * Fields in order: text full width with whose words it is, a sheet's rows as
 * a table, everything else on rows. Text inside an 84px-label row reads as a
 * value; a message is the thing being approved.
 */
function Flow({ fields }: { fields: Field[] }) {
  const runs: Field[][] = [];
  for (const field of fields) {
    const last = runs[runs.length - 1];
    if (!wide(field) && last && !wide(last[0])) last.push(field);
    else runs.push([field]);
  }
  return (
    <>
      {runs.map((run) =>
        run[0].cells.length > 0 ? (
          <SheetTable key={run[0].path} field={run[0]} />
        ) : run[0].role === "text" ? (
          <TextBlock key={run[0].path} field={run[0]} />
        ) : (
          <FieldRows key={run[0].path} fields={run} />
        ),
      )}
    </>
  );
}

/** The record's own name, large, as the line its app opens on. */
function Heading({ fields }: { fields: Field[] }) {
  return (
    <>
      {fields.map((f) => (
        <p className="preview__lead preview__heading preview__heading--big" key={f.path}>
          <HeadingText field={f} />
        </p>
      ))}
    </>
  );
}

/**
 * An email as a compose window: who it goes to on hairlines, the subject as a
 * plain line, then the body — no labels on either, just as Mail and Gmail.
 */
function Compose({ preview }: { preview: ActionPreview }) {
  const [people, other] = split(preview.fields, "person", "destination");
  const [subject, rest1] = split(other, "heading");
  const [body, rest] = split(rest1, "text");
  return (
    <>
      {people.length > 0 && (
        <div className="preview__compose">
          <FieldRows fields={people} />
        </div>
      )}
      {subject.map((f) => (
        <p className="preview__lead" key={f.path}>
          <HeadingText field={f} />
        </p>
      ))}
      {body.map((f) => (
        <TextBlock key={f.path} field={f} />
      ))}
      <Flow fields={rest} />
    </>
  );
}

/** Who a chat message goes to: its channel or chat, else a person. */
function recipient(preview: ActionPreview): Field | undefined {
  return (
    preview.fields.find((f) => f.role === "destination") ??
    preview.fields.find((f) => f.role === "person")
  );
}

/** Where messages go to a channel by name, the name wears its "#". */
const CHANNEL_APPS = /slack|discord/i;

/** A chat's window is titled by who it is with: "#launch", "Priya". */
function chatTitle(preview: ActionPreview, app: string): ReactNode {
  const to = recipient(preview);
  if (!to) return "New Message";
  if (to.unresolved && !to.name) return <UnresolvedId value={to.value} />;
  const name = to.name ?? to.value;
  const sigil = to.role === "destination" && CHANNEL_APPS.test(app) && !/^[#@]/.test(name);
  return (
    <>
      {sigil && <span className="preview__sigil">#</span>}
      {name}
    </>
  );
}

/** A chat message as the bubble you send; who it goes to is the window's title. */
function Chat({ preview }: { preview: ActionPreview }) {
  const to = recipient(preview);
  const [text, rest] = split(
    preview.fields.filter((f) => f !== to),
    "text",
  );
  return (
    <>
      {text.map((f) => (
        <Bubble key={f.path} field={f} />
      ))}
      <Flow fields={rest} />
    </>
  );
}

/** When is what you check before an event saves, so it leads, large. */
function Event({ preview }: { preview: ActionPreview }) {
  const [heading, other] = split(preview.fields, "heading");
  const when = other.find((f) => f.role === "when");
  const [people, rest] = split(
    other.filter((f) => f !== when),
    "person",
  );
  return (
    <>
      <Heading fields={heading} />
      {when && (
        <p className="preview__lead">
          <WhenBlock big when={when.value} under={when.items[0]} />
        </p>
      )}
      <FieldRows fields={people} />
      <Flow fields={rest} />
    </>
  );
}

/** A reminder as its list draws one: an empty circle, what to do, when. */
function Task({ preview }: { preview: ActionPreview }) {
  const item =
    preview.fields.find((f) => f.role === "text") ??
    preview.fields.find((f) => f.role === "heading");
  const due = preview.fields.find((f) => f.role === "when");
  const rest = preview.fields.filter((f) => f !== item && f !== due);
  return (
    <>
      <div className="preview__lead preview__task">
        <span className="preview__check" aria-hidden />
        <div className="preview__task-body">
          {item && <MessageBody field={item} />}
          {due && <span className="preview__due">{due.value}</span>}
          {item?.origin && <OriginTag origin={item.origin} />}
        </div>
      </div>
      <Flow fields={rest} />
    </>
  );
}

/** A note: its title in bold, then what it says. */
function Note({ preview }: { preview: ActionPreview }) {
  const [heading, other] = split(preview.fields, "heading");
  const [text, rest] = split(other, "text");
  return (
    <>
      <Heading fields={heading} />
      {text.map((f) => (
        <TextBlock key={f.path} field={f} />
      ))}
      <Flow fields={rest} />
    </>
  );
}

/** An issue: its title, what is wrong, then its fields as chips. */
function Issue({ preview }: { preview: ActionPreview }) {
  const [heading, other] = split(preview.fields, "heading");
  const [text, rest] = split(other, "text");
  return (
    <>
      <Heading fields={heading} />
      {text.map((f) => (
        <TextBlock key={f.path} field={f} />
      ))}
      <Tags fields={rest} />
    </>
  );
}

/** A row bound for a sheet, in the sheet's own columns. */
function Sheet({ preview }: { preview: ActionPreview }) {
  return <Flow fields={preview.fields} />;
}

/** A move as Finder's rows: each file, then the folder it goes into. */
function Files({ preview }: { preview: ActionPreview }) {
  const from = preview.fields.find((f) => f.role === "list");
  const to = preview.fields.find((f) => f.role === "path");
  if (!from || !to) return <Flow fields={preview.fields} />;
  const items = from.items.length > 0 ? from.items : [from.value];
  const more = from.count - items.length;
  return (
    <>
      <div className="preview__lead preview__finder">
        {items.map((path, i) => (
          // A fixed list in a fixed order; two paths may read alike.
          <span className="preview__finder-row" key={i}>
            <FileGlyph name={path} />
            <span className="preview__path">{path}</span>
          </span>
        ))}
        {more > 0 && <span className="preview__faint">and {more} more</span>}
        <span className="preview__finder-row">
          <ArrowRightIcon className="file-glyph" size={14} />
          <FileGlyph name={to.value} folder />
          <span className="preview__path">{to.value}</span>
        </span>
      </div>
      <Flow fields={preview.fields.filter((f) => f !== from && f !== to)} />
    </>
  );
}

/** A share: the document, who gets it, and as what. */
function Share({ preview }: { preview: ActionPreview }) {
  const doc =
    preview.fields.find((f) => f.role === "id") ??
    preview.fields.find((f) => f.role === "path");
  const [people, other] = split(
    preview.fields.filter((f) => f !== doc),
    "person",
  );
  const [roles, rest] = split(other, "pill");
  return (
    <>
      {doc && (
        <p className="preview__lead preview__finder-row">
          <FileGlyph name={doc.name ?? doc.value} />
          <span className="preview__heading">
            <Value field={doc} />
          </span>
        </p>
      )}
      {people.length > 0 && (
        <p className="preview__lead preview__chips">
          {people.map((f) => (
            <Value key={f.path} field={f} />
          ))}
        </p>
      )}
      <Tags fields={roles} />
      <Flow fields={rest} />
    </>
  );
}

/** Each changed field before and after; the rest as they are. */
function Update({ preview }: { preview: ActionPreview }) {
  return (
    <>
      {preview.fields.map((f) =>
        f.before != null ? (
          <DiffRow key={f.path} field={f} />
        ) : (
          <Flow key={f.path} fields={[f]} />
        ),
      )}
    </>
  );
}

/**
 * What goes, by example: how the items were picked, the first few as a read
 * showed them, and whether any of it comes back.
 */
function Delete({ preview }: { preview: ActionPreview }) {
  const [lists, rest] = split(preview.fields, "list");
  return (
    <>
      {preview.pickedBy && (
        <p className="preview__lead">
          <span className="preview__faint">Matched: {preview.pickedBy}</span>
        </p>
      )}
      {lists.map((f) => (
        <ItemSample key={f.path} field={f} />
      ))}
      <Flow fields={rest} />
      {preview.recoverable && <RiskLine text={preview.recoverable} />}
    </>
  );
}

/** The amount leads, large and in real units; who and how follow. */
function Money({ preview }: { preview: ActionPreview }) {
  const [money, rest] = split(preview.fields, "money");
  return (
    <>
      {money.map((f) => (
        <p className="preview__lead" key={f.path}>
          <MoneyLine amount={f.value} />
        </p>
      ))}
      <Flow fields={rest} />
    </>
  );
}

/** Straight from the schema: every set field on a row, and how many were set. */
function Fallback({ preview }: { preview: ActionPreview }) {
  return <FieldRows fields={preview.fields} />;
}

/** A kind with no window of its own in its app is titled by what it does. */
const own = (preview: ActionPreview) => preview.title;

const LAYOUTS: Record<CardKind, Layout> = {
  email: { title: () => "New Message", body: Compose },
  message: { title: chatTitle, body: Chat },
  event: { title: () => "New Event", body: Event },
  task: { title: (_, app) => (/reminders/i.test(app) ? "New Reminder" : "New Task"), body: Task },
  note: { title: () => "New Note", body: Note },
  issue: { title: () => "New Issue", body: Issue },
  row: { title: own, body: Sheet },
  file: { title: own, body: Files },
  share: { title: own, body: Share },
  update: { title: own, body: Update },
  delete: { title: own, body: Delete },
  money: { title: own, body: Money },
  fallback: { title: own, body: Fallback },
};

/** The window that draws a kind. Exported for the test that pins the table. */
export function layoutFor(kind: CardKind): Layout {
  return LAYOUTS[kind];
}

/**
 * What a finished step's echo keeps of its card: the field that names what
 * was written, ahead of who it went to, which the title mostly says already.
 */
const KEY_ROLES: Role[] = [
  "heading",
  "text",
  "money",
  "when",
  "list",
  "path",
  "id",
  "destination",
  "person",
];

function keyField(preview: ActionPreview): Field | undefined {
  for (const role of KEY_ROLES) {
    // An id nothing named is no name for anything.
    const found = preview.fields.find((f) => f.role === role && (role !== "id" || f.name));
    if (found) return found;
  }
  return preview.fields[0];
}

/**
 * The card for a write, as a small window of the app it writes to. `compact`
 * is the finished step's echo of it: the same window, its title bar and one
 * line. `actions` are the gate's answers, at the window's foot where the app
 * keeps its own Send.
 */
export function PreviewCard({
  preview,
  app = "",
  icon = null,
  compact = false,
  actions,
}: {
  preview: ActionPreview;
  app?: string;
  icon?: string | null;
  compact?: boolean;
  actions?: ReactNode;
}) {
  const layout = layoutFor(preview.kind);
  const Body = layout.body;
  const key = compact ? keyField(preview) : undefined;
  return (
    <div
      className={compact ? "preview preview--compact" : "preview"}
      data-kind={preview.kind}
      data-strict={preview.tier === "strict" || undefined}
    >
      <div className="preview__bar">
        <AppIcon app={app} icon={icon} />
        <span className="preview__title">{layout.title(preview, app)}</span>
      </div>
      {compact ? (
        key && <p className="preview__key">{key.name ?? key.value}</p>
      ) : (
        <div className="preview__content">
          <Body preview={preview} />
          <MoreFields fields={preview.more} />
          {preview.risks.map((risk) => (
            <RiskLine key={risk} text={risk} danger={preview.tier === "strict"} />
          ))}
          {preview.safer && <SaferLine label={preview.safer.label} />}
          {preview.kind === "fallback" && preview.total > 0 && (
            <p className="preview__foot">
              {preview.set} of {preview.total} fields set
            </p>
          )}
        </div>
      )}
      {actions && <div className="preview__actions">{actions}</div>}
    </div>
  );
}
