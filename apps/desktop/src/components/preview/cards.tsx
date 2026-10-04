import type { ReactNode } from "react";
import type { ActionPreview, CardKind, Field, Role } from "@/lib/bindings";
import {
  DiffRow,
  FieldRows,
  HeadingText,
  ItemSample,
  MoneyLine,
  MoreFields,
  PathMove,
  RiskLine,
  TextBlock,
  WhenBlock,
} from "./kit";

/**
 * One layout per kind of action, all arrangements of the kit. Rust picks the
 * kind and fills every field; a layout only decides what leads and what sits
 * on a row. It never formats a value and never drops a field.
 */

type Layout = (props: { preview: ActionPreview }) => ReactNode;

/** The fields with one of `roles`, and the rest, each in their own order. */
function split(fields: Field[], ...roles: Role[]): [Field[], Field[]] {
  return [
    fields.filter((f) => roles.includes(f.role)),
    fields.filter((f) => !roles.includes(f.role)),
  ];
}

/**
 * Fields in order: text full width with whose words it is, everything else on
 * rows. Text inside an 84px-label row reads as a value; a message is the
 * thing being approved.
 */
function Flow({ fields }: { fields: Field[] }) {
  const runs: Field[][] = [];
  for (const field of fields) {
    const last = runs[runs.length - 1];
    if (field.role !== "text" && last && last[0].role !== "text")
      last.push(field);
    else runs.push([field]);
  }
  return (
    <>
      {runs.map((run) =>
        run[0].role === "text" ? (
          <TextBlock key={run[0].path} field={run[0]} />
        ) : (
          <FieldRows key={run[0].path} fields={run} />
        ),
      )}
    </>
  );
}

/** The record's own name, as a line rather than a "Title" row. */
function Heading({ fields }: { fields: Field[] }) {
  return (
    <>
      {fields.map((f) => (
        <p className="preview__lead preview__heading" key={f.path}>
          <HeadingText field={f} />
        </p>
      ))}
    </>
  );
}

/** Message and Email: who it goes to, then the words, whole. */
function Message({ preview }: { preview: ActionPreview }) {
  return <Flow fields={preview.fields} />;
}

/** Task and Note: what is being written down leads; where and when follow. */
function Item({ preview }: { preview: ActionPreview }) {
  const [heading, other] = split(preview.fields, "heading");
  const [content, rest] = split(other, "text");
  return (
    <>
      <Heading fields={heading} />
      <Flow fields={[...content, ...rest]} />
    </>
  );
}

/** Issue, Row and Share: the record by name, its body, then each field. */
function Entry({ preview }: { preview: ActionPreview }) {
  const [heading, rest] = split(preview.fields, "heading");
  return (
    <>
      <Heading fields={heading} />
      <Flow fields={rest} />
    </>
  );
}

/** When is what you check before an event saves, so it leads, large. */
function Event({ preview }: { preview: ActionPreview }) {
  const [heading, other] = split(preview.fields, "heading");
  const when = other.find((f) => f.role === "when");
  return (
    <>
      <Heading fields={heading} />
      {when && (
        <p className="preview__lead">
          <WhenBlock big when={when.value} />
        </p>
      )}
      <Flow fields={other.filter((f) => f !== when)} />
    </>
  );
}

/** Where the files are and where they go, as one move. */
function File({ preview }: { preview: ActionPreview }) {
  const from = preview.fields.find((f) => f.role === "list");
  const to = preview.fields.find((f) => f.role === "path");
  if (!from || !to) return <Flow fields={preview.fields} />;
  return (
    <>
      <p className="preview__lead">
        <PathMove from={from.value} to={to.value} />
      </p>
      <Flow fields={preview.fields.filter((f) => f !== from && f !== to)} />
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

const LAYOUTS: Record<CardKind, Layout> = {
  message: Message,
  email: Message,
  task: Item,
  note: Item,
  issue: Entry,
  row: Entry,
  share: Entry,
  event: Event,
  file: File,
  update: Update,
  delete: Delete,
  money: Money,
  fallback: Fallback,
};

/** The layout that draws a kind. Exported for the test that pins the table. */
export function layoutFor(kind: CardKind): Layout {
  return LAYOUTS[kind];
}

/**
 * The card for a write. `compact` is the finished step's echo of it: the
 * layout alone, without the folded fields and the warnings that only matter
 * before saying yes.
 */
export function PreviewCard({
  preview,
  compact = false,
}: {
  preview: ActionPreview;
  compact?: boolean;
}) {
  const Layout = layoutFor(preview.kind);
  return (
    <div
      className={compact ? "preview preview--compact" : "preview"}
      data-kind={preview.kind}
    >
      <Layout preview={preview} />
      {!compact && (
        <>
          <MoreFields fields={preview.more} />
          {preview.risks.map((risk) => (
            <RiskLine
              key={risk}
              text={risk}
              danger={preview.tier === "strict"}
            />
          ))}
          {preview.kind === "fallback" && preview.total > 0 && (
            <p className="preview__foot">
              {preview.set} of {preview.total} fields set
            </p>
          )}
        </>
      )}
    </div>
  );
}
