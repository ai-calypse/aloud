import { ChevronRightIcon } from "lucide-react";
import type { ActionPreview, Field } from "@/lib/bindings";
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

export function WhenBlock({ when }: { when: string }) {
  return <span className="preview__when">{when}</span>;
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
      return <MessageBody text={field.value} />;
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

/**
 * The card for a tool nobody has drawn one for yet, straight from its schema:
 * plainer than a hand-made view, but never blank.
 */
export function FallbackCard({ preview }: { preview: ActionPreview }) {
  return (
    <div className="preview">
      <FieldRows fields={preview.fields} />
      <MoreFields fields={preview.more} />
      {preview.total > 0 && (
        <p className="preview__foot">
          {preview.set} of {preview.total} fields set
        </p>
      )}
    </div>
  );
}
