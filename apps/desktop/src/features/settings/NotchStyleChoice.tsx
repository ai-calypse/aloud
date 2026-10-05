import type { NotchStyle } from "@/lib/bindings";
import { cn } from "@/lib/utils";

const CHOICES: { id: NotchStyle; name: string; line: string }[] = [
  { id: "calm", name: "Calm", line: "Plain black and white, the way the notch has always looked." },
  { id: "widgets", name: "Widgets", line: "Colourful tiles like the widgets on a Mac's desktop." },
];

/**
 * Two cards, one choice. Native radios under the cards, not buttons: the
 * arrow keys, the group role and the checked state come from the platform,
 * and `peer` lets the card draw the checked and focus states from them.
 */
export function NotchStyleChoice({
  value,
  onChange,
}: {
  value: NotchStyle;
  onChange: (style: NotchStyle) => void;
}) {
  return (
    <fieldset className="m-0 grid grid-cols-2 gap-2.5 border-0 border-b-[0.5px] border-solid border-separator p-0 py-3">
      <legend className="sr-only">Notch look</legend>
      {CHOICES.map(({ id, name, line }) => (
        <label key={id} className="relative block cursor-pointer">
          <input
            type="radio"
            name="notch-style"
            value={id}
            checked={value === id}
            onChange={() => onChange(id)}
            className="peer sr-only"
          />
          <span
            className={cn(
              "block rounded-field border border-separator bg-field p-3 text-left",
              "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-out)]",
              "peer-checked:border-brand peer-checked:bg-hover",
              "peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-brand",
            )}
          >
            <span className="block text-row font-medium">{name}</span>
            <span className="mt-px block text-xs text-ink-2">{line}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
