import { createContext, useContext } from "react";
import {
  AppWindowIcon,
  ChevronRightIcon,
  FileIcon,
  FileSpreadsheetIcon,
  FileTextIcon,
  FolderIcon,
} from "lucide-react";
import type { Field, Origin } from "@/lib/bindings";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

/**
 * The pieces an approval card is made of. Every string arrives formatted from
 * Rust (`aloud_agent::preview`), so nothing here parses, formats or guesses —
 * and it is all plain text: the arguments are whatever the model wrote, and a
 * card that rendered them as markdown or loaded an image URL from them would
 * show something other than what is about to be sent.
 */

/**
 * The open gate's draft, keyed by each field's pointer. Provided only around
 * the card waiting on a yes, so a finished step's echo — and any card Rust did
 * not mark `editable` — stays plain text.
 */
export type CardEdits = {
  draft: Record<string, string>;
  edit: (path: string, value: string) => void;
  /** Focus on a field holds the countdown: the user is reading or changing it. */
  hold: () => void;
};

export const CardEditsContext = createContext<CardEdits | null>(null);

/** The draft to edit `field` in, or null where it is shown as text. */
function useEditing(field: Field): CardEdits | null {
  const edits = useContext(CardEditsContext);
  return field.editable ? edits : null;
}

/**
 * The acting app, as its own icon — the Mac app's, or the service's logo,
 * always a `data:` URI from Rust (the overlay loads no other image). Without
 * one, a neutral window glyph: never a guess at another app's mark.
 */
export function AppIcon({ app, icon }: { app: string; icon?: string | null }) {
  return icon ? (
    <img className="app-icon" src={icon} alt={app} draggable={false} />
  ) : (
    <span className="app-icon app-icon--glyph" role="img" aria-label={app}>
      <AppWindowIcon size={12} />
    </span>
  );
}

/** Spreadsheets and text read apart at a glance in Finder; everything else is a file. */
const SHEET = /\.(xlsx?|csv|numbers|tsv)$/i;
const TEXT = /\.(pdf|docx?|pages|txt|md|rtf|key|pptx?)$/i;

/** A file's type as Finder's list would mark it, or a folder. */
export function FileGlyph({ name, folder }: { name: string; folder?: boolean }) {
  if (folder) return <FolderIcon className="file-glyph file-glyph--folder" size={14} />;
  const Glyph = SHEET.test(name) ? FileSpreadsheetIcon : TEXT.test(name) ? FileTextIcon : FileIcon;
  return <Glyph className="file-glyph" size={14} />;
}

/**
 * A field taken as typed: a single line in its row. An empty one is what the
 * model could not fill — an address it never found — so it takes focus. Who
 * it goes to stays a chip while it is being corrected.
 */
function FieldInput({ field, edits }: { field: Field; edits: CardEdits }) {
  const chip = field.role === "person" || field.role === "destination";
  return (
    <Input
      className={chip ? "preview__input preview__input--chip" : "preview__input"}
      value={edits.draft[field.path] ?? field.value}
      aria-label={field.label}
      placeholder={field.label}
      autoFocus={field.value === ""}
      onFocus={edits.hold}
      onChange={(e) => edits.edit(field.path, e.target.value)}
    />
  );
}

/** Where a message goes, by name. An id nothing has named is flagged, not shown as one. */
export function DestinationChip({ field }: { field: Field }) {
  if (field.unresolved && !field.name) return <UnresolvedId value={field.value} />;
  return <span className="preview__chip">{field.name ?? field.value}</span>;
}

/** An address in full: the domain is the part that says who it really is. */
export function PersonChip({ address }: { address: string }) {
  return <span className="preview__chip">{address}</span>;
}

/**
 * The text being sent, whole. Never truncated: it is the thing being approved.
 * On the open card it is the field itself — fix a word in place, ⌘↩ to send.
 */
export function MessageBody({ field }: { field: Field }) {
  const edits = useEditing(field);
  if (!edits) return <p className="preview__body">{field.value}</p>;
  return (
    <Textarea
      className="preview__body preview__input"
      value={edits.draft[field.path] ?? field.value}
      aria-label={field.label}
      onFocus={edits.hold}
      onChange={(e) => edits.edit(field.path, e.target.value)}
    />
  );
}

/**
 * Whose words these are. "Your words" only when the user said them whole;
 * anything the model wrote or reworded is marked as Aloud's, because that is
 * the text worth reading twice before it goes out.
 */
export function OriginTag({ origin }: { origin: Origin }) {
  return origin === "yours" ? (
    <span className="preview__origin preview__origin--yours">your words</span>
  ) : (
    <span className="preview__origin preview__origin--aloud">written by Aloud</span>
  );
}

/** A text field on its own line, full width, with whose words it is. */
export function TextBlock({ field }: { field: Field }) {
  return (
    <div className="preview__text">
      <MessageBody field={field} />
      {field.origin && <OriginTag origin={field.origin} />}
    </div>
  );
}

/** A message as its chat app draws one you send: a bubble, editable in place. */
export function Bubble({ field }: { field: Field }) {
  return (
    <div className="preview__chat">
      <div className="preview__bubble">
        <MessageBody field={field} />
      </div>
      {field.origin && <OriginTag origin={field.origin} />}
    </div>
  );
}

/**
 * Fields as labelled chips, the way an issue tracker shows priority, assignee
 * and labels under the description.
 */
export function Tags({ fields }: { fields: Field[] }) {
  if (fields.length === 0) return null;
  return (
    <div className="preview__tags">
      {fields.map((field) => (
        <span className="preview__tag" key={field.path}>
          <span className="preview__faint">{field.label}</span>
          <Value field={field} />
        </span>
      ))}
    </div>
  );
}

/** A record's own name: its title or subject, editable on the open card. */
export function HeadingText({ field }: { field: Field }) {
  const edits = useEditing(field);
  return edits ? <FieldInput field={field} edits={edits} /> : <>{field.value}</>;
}

/** A time, and on an event the zone and length on one line under it. */
export function WhenBlock({ when, under, big }: { when: string; under?: string; big?: boolean }) {
  return (
    <>
      <span className={big ? "preview__when preview__when--big" : "preview__when"}>{when}</span>
      {under && <span className="preview__under">{under}</span>}
    </>
  );
}

/**
 * Rows being written to a sheet, drawn as the sheet will hold them: column
 * letters over the cells, so a value in the wrong column shows as one.
 */
export function SheetTable({ field }: { field: Field }) {
  const [head, ...rows] = field.cells;
  const rest = field.count - rows.length;
  return (
    <div className="preview__lead preview__sheet">
      <table>
        <thead>
          <tr>
            {head.map((letter) => (
              <th key={letter}>{letter}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            // Fixed rows in a fixed order; two rows may hold the same cells.
            <tr key={i}>
              {row.map((cell, j) => (
                <td key={head[j]}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {rest > 0 && <span className="preview__faint">and {rest} more rows</span>}
    </div>
  );
}

/**
 * One field an update changes: what it is now, struck through, and what it
 * becomes. A field whose current value no read returned shows only the new
 * one — never a guessed "before".
 */
export function DiffRow({ field }: { field: Field }) {
  return (
    <div className="preview__row">
      <span className="preview__label">{field.label}</span>
      <span className="preview__value">
        {field.before != null && (
          <>
            <span className="preview__old">{field.before}</span>
            <span className="preview__arrow">→</span>
          </>
        )}
        <span className="preview__new">{field.name ?? field.value}</span>
      </span>
    </div>
  );
}

/**
 * What a bulk action acts on: the first few items as a read showed them, and
 * how many more. Each item is "who · what" when the read gave both.
 */
export function ItemSample({ field }: { field: Field }) {
  const items = field.items.length > 0 ? field.items : [field.value];
  const rest = field.count - items.length;
  return (
    <div className="preview__sample">
      {items.map((item, i) => {
        const [who, ...what] = item.split(" · ");
        return (
          // A fixed sample that never reorders, and two items may read alike.
          <div className="preview__item" key={i}>
            {what.length > 0 ? (
              <>
                <span className="preview__faint">{who}</span>
                <span>{what.join(" · ")}</span>
              </>
            ) : (
              <span>{item}</span>
            )}
          </div>
        );
      })}
      {rest > 0 && <span className="preview__faint">and {rest} more</span>}
    </div>
  );
}

/** One consequence in a sentence: who hears about it, whether it can be undone. */
export function RiskLine({ text, danger }: { text: string; danger?: boolean }) {
  return <p className={danger ? "preview__risk preview__risk--danger" : "preview__risk"}>{text}</p>;
}

/**
 * The gentler tool for the same job, by name. A line rather than a button:
 * saying no and asking for it is the way to get it, so the gate keeps one
 * answer per question.
 */
export function SaferLine({ label }: { label: string }) {
  return <p className="preview__risk">Safer: {label} — cancel and ask for that instead.</p>;
}

/** A file's path, or where it goes from and to when the call names both. */
export function PathMove({ from, to }: { from?: string; to: string }) {
  return (
    <span className="preview__path">
      {from && (
        <>
          {from}
          <br />→{" "}
        </>
      )}
      {to}
    </span>
  );
}

/** Already in major units with its currency; never the raw minor-unit integer. */
export function MoneyLine({ amount }: { amount: string }) {
  return <span className="preview__money">{amount}</span>;
}

/** An identifier no lookup has named. Shown and flagged, so it is never mistaken for a name. */
export function UnresolvedId({ value }: { value: string }) {
  return (
    <span className="preview__id">
      <span className="preview__path">{value}</span>
      <span className="preview__flag">not looked up</span>
    </span>
  );
}

export function Value({ field }: { field: Field }) {
  const edits = useEditing(field);
  if (edits && field.role !== "text") return <FieldInput field={field} edits={edits} />;
  switch (field.role) {
    case "destination":
      return <DestinationChip field={field} />;
    case "id":
      return field.name ? <>{field.name}</> : <UnresolvedId value={field.value} />;
    case "person": {
      // Who Contacts resolved a spoken name to, then the address it sends to.
      if (field.name)
        return (
          <span className="preview__chips">
            <PersonChip address={field.name} />
            <span className="preview__faint">{field.value}</span>
          </span>
        );
      const people = field.items.length > 0 ? field.items : [field.value];
      const rest = field.count - field.items.length;
      return (
        <span className="preview__chips">
          {people.map((address, i) => (
            // A static list that never reorders, and two equal addresses are possible.
            <PersonChip key={i} address={address} />
          ))}
          {rest > 0 && <span className="preview__faint">and {rest} more</span>}
        </span>
      );
    }
    case "text":
      return <TextBlock field={field} />;
    case "when":
      return <WhenBlock when={field.value} />;
    case "money":
      return <MoneyLine amount={field.value} />;
    case "path":
      return <PathMove to={field.value} />;
    case "url":
      // The full host is the line that matters; the whole URL under it.
      return (
        <>
          <span className="preview__heading">{field.value}</span>
          {field.items[0] && <span className="preview__faint"> {field.items[0]}</span>}
        </>
      );
    case "heading":
      return <span className="preview__heading">{field.value}</span>;
    case "pill":
      return <span className="preview__chip">{field.value}</span>;
    default:
      return <>{field.value}</>;
  }
}

/** Label and value on a hairline each, labels from the schema's own titles. */
export function FieldRows({ fields }: { fields: Field[] }) {
  // Two paths, one of them a source, are a single move rather than two
  // unrelated rows.
  const paths = fields.filter((f) => f.role === "path");
  const from =
    paths.length === 2
      ? paths.find((f) => /from|source|src|old/i.test(f.path))
      : undefined;
  const to = from && paths.find((f) => f !== from);
  return (
    <>
      {fields.map((field) =>
        field === to ? null : (
          <div className="preview__row" key={field.path}>
            <span className="preview__label">{field === from ? "Move" : field.label}</span>
            <span className="preview__value">
              {field === from && to ? (
                <PathMove from={from.value} to={to.value} />
              ) : (
                <Value field={field} />
              )}
            </span>
          </div>
        ),
      )}
    </>
  );
}

/** Every other argument the call sets: folded away, never dropped. */
export function MoreFields({ fields }: { fields: Field[] }) {
  if (fields.length === 0) return null;
  return (
    <Collapsible>
      <CollapsibleTrigger className="preview__more">
        <ChevronRightIcon className="preview__chevron" size={12} />
        {fields.length} more
      </CollapsibleTrigger>
      <CollapsibleContent>
        <FieldRows fields={fields} />
      </CollapsibleContent>
    </Collapsible>
  );
}
