import { ChevronRightIcon } from "lucide-react";
import type { Field, Origin } from "@/lib/bindings";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";

/**
 * The pieces an approval card is made of. Every string arrives formatted from
 * Rust (`aloud_agent::preview`), so nothing here parses, formats or guesses —
 * and it is all plain text: the arguments are whatever the model wrote, and a
 * card that rendered them as markdown or loaded an image URL from them would
 * show something other than what is about to be sent.
 */

/** Where a message goes, by name. An id nothing has named is flagged, not shown as one. */
export function DestinationChip({ field }: { field: Field }) {
  if (field.unresolved && !field.name) return <UnresolvedId value={field.value} />;
  return <span className="preview__chip">{field.name ?? field.value}</span>;
}

/** An address in full: the domain is the part that says who it really is. */
export function PersonChip({ address }: { address: string }) {
  return <span className="preview__chip">{address}</span>;
}

/** The text being sent, whole. Never truncated: it is the thing being approved. */
export function MessageBody({ text }: { text: string }) {
  return <p className="preview__body">{text}</p>;
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
      <MessageBody text={field.value} />
      {field.origin && <OriginTag origin={field.origin} />}
    </div>
  );
}

export function WhenBlock({ when, big }: { when: string; big?: boolean }) {
  return <span className={big ? "preview__when preview__when--big" : "preview__when"}>{when}</span>;
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

function Value({ field }: { field: Field }) {
  switch (field.role) {
    case "destination":
      return <DestinationChip field={field} />;
    case "id":
      return field.name ? <>{field.name}</> : <UnresolvedId value={field.value} />;
    case "person": {
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
