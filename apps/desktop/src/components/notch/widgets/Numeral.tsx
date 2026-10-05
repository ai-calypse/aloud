import { cn } from "@/lib/utils";
import { numeralParts, type NumeralPart } from "./math";

const SIZE = {
  s: "text-notch-w-numeral-s",
  m: "text-notch-w-numeral-m",
  l: "text-notch-w-numeral-l",
  xl: "text-notch-w-numeral-xl",
} as const;

/** Widgets sit tighter than SF's default; Calm's Geist numerals tighter still. */
const TRACKING = {
  widget: "tracking-notch-w-numeral",
  calm: "tracking-notch-n",
} as const;

/**
 * A big number in Apple's way: Light 300, tabular, tight. `text` is already
 * formatted (see `formatNumeral`), so the caller decides decimals and units.
 *
 * With `roll`, each digit is a column that slides to its place on the
 * spring timing, so 59 → 60 turns over rather than swapping. It is CSS and
 * the targets are whole digits a tick apart, so there is nothing to
 * retarget; reduced motion collapses the slide to a jump through the global
 * switch in motion.css. The text is spoken once, whole, not digit by digit.
 */
export function Numeral({
  text,
  size = "l",
  roll = false,
  tracking = "widget",
  className,
}: {
  text: string;
  size?: keyof typeof SIZE;
  roll?: boolean;
  tracking?: keyof typeof TRACKING;
  className?: string;
}) {
  const base = cn(
    "font-light leading-none tabular-nums text-widget-ink",
    TRACKING[tracking],
    SIZE[size],
    className,
  );
  if (!roll) return <span className={base}>{text}</span>;

  const parts = numeralParts(text);
  return (
    <span className={cn(base, "notch-numeral")}>
      <span className="sr-only">{text}</span>
      <span aria-hidden className="inline-flex">
        {parts.map((part, i) => (
          // Keyed from the right, so a digit gained or lost (9 → 10) leaves
          // the ones place where it was rather than re-rolling every column.
          <Part key={parts.length - i} part={part} />
        ))}
      </span>
    </span>
  );
}

function Part({ part }: { part: NumeralPart }) {
  if (part.kind === "char") return <span className="notch-numeral__cell">{part.char}</span>;
  return (
    <span className="notch-numeral__digit">
      <span
        className="block transition-transform duration-[var(--spring-dur)] ease-[var(--spring)]"
        style={{ transform: `translateY(${-part.digit}em)` }}
      >
        {/* Ten digits stacked; cells are 1em tall, so this holds at any size. */}
        {DIGITS.map((d) => (
          <span key={d} className="notch-numeral__cell">
            {d}
          </span>
        ))}
      </span>
    </span>
  );
}

const DIGITS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
