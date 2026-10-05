/**
 * The arithmetic behind the widget primitives, apart from React so the parts
 * that can silently be wrong (a ring drawn the wrong length, a digit rolled
 * to the wrong place) are testable on their own.
 */

/** 0…1, whatever arrives. A NaN draws empty rather than a full ring. */
export function unit(value: number): number {
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

/** The centre-line radius and circumference of a ring stroked `stroke` wide in a `size` box. */
export function ringGeometry(size: number, stroke: number) {
  const r = Math.max(0, (size - stroke) / 2);
  return { r, circumference: 2 * Math.PI * r };
}

/** How much of the circumference is left undrawn at `value`: 0 is a full ring. */
export function dashOffset(circumference: number, value: number): number {
  return circumference * (1 - unit(value));
}

/**
 * Concentric rings, outermost first: Fitness's Move, Exercise, Stand. Each
 * ring's stroke is the same, with a small gap between, and the innermost
 * keeps a hole in the middle; `size` is the outer diameter.
 */
export function stackGeometry(size: number, count: number) {
  const n = Math.max(1, Math.floor(count));
  const stroke = size / (2 * n + 2);
  const gap = stroke * 0.25;
  return Array.from({ length: n }, (_, i) => {
    const diameter = size - 2 * i * (stroke + gap);
    return { size: diameter, stroke, ...ringGeometry(diameter, stroke) };
  });
}

/** How many of `count` segments are lit at `value`: a partial one rounds to the nearest. */
export function litSegments(value: number, count: number): number {
  return Math.round(unit(value) * Math.max(0, count));
}

/**
 * A number as a numeral reads: fixed decimals, and a dash for what has no
 * value (Rust's NaN reaches the webview as null). ASCII digits always, so a
 * digit roller can place each one; grouping is opt-in.
 */
export function formatNumeral(
  value: number | null | undefined,
  { decimals = 0, group = false }: { decimals?: number; group?: boolean } = {},
): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "–";
  const fixed = Math.abs(value).toFixed(decimals);
  const [whole, fraction] = fixed.split(".");
  const grouped = group ? whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",") : whole;
  const text = fraction ? `${grouped}.${fraction}` : grouped;
  // A rounded-away negative is not "-0".
  return value < 0 && Number(fixed) !== 0 ? `−${text}` : text;
}

export type NumeralPart = { kind: "digit"; digit: number } | { kind: "char"; char: string };

/** A formatted numeral split into rollable digits and fixed characters. */
export function numeralParts(text: string): NumeralPart[] {
  return Array.from(text, (char) =>
    char >= "0" && char <= "9"
      ? { kind: "digit", digit: Number(char) }
      : { kind: "char", char },
  );
}

/**
 * The polyline of a sparkline in a `width` x `height` box, as an SVG path.
 * Scaled to the values' own range (a flat series draws mid-height rather than
 * dividing by zero), with `pad` kept clear so the stroke is not clipped.
 */
export function sparkPath(values: number[], width: number, height: number, pad = 1.5): string {
  const finite = values.filter(Number.isFinite);
  if (finite.length < 2) return "";
  const min = Math.min(...finite);
  const span = Math.max(...finite) - min;
  const x = (i: number) => (i / (finite.length - 1)) * width;
  const y = (v: number) =>
    span === 0 ? height / 2 : height - pad - ((v - min) / span) * (height - 2 * pad);
  return finite.map((v, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(" ");
}
