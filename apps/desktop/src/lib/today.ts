import type { MarketState, Ticker } from "@/lib/bindings";

/**
 * Fahrenheit for the few regions that use it. The locale's region, not the
 * language: `en-GB` and `en-CA` are English and Celsius.
 */
export function usesFahrenheit(locale: string): boolean {
  let region: string | undefined;
  try {
    region = new Intl.Locale(locale).region;
  } catch {
    return false;
  }
  return region === "US" || region === "LR" || region === "MM";
}

/**
 * `21°`, `-3°`. Rounds, and never shows `-0°`. Rust's f64 reaches the
 * webview as `number | null` (a NaN serialises to null), hence the dash.
 */
export function degrees(value: number | null): string {
  if (value === null) return "–";
  const n = Math.round(value);
  return `${n === 0 ? 0 : n}°`;
}

/** What a quote's state says beside its price; nothing while it is live. */
export function marketLabel(t: Pick<Ticker, "state" | "delayed">): string {
  if (t.delayed) return "delayed";
  const labels: Record<MarketState, string> = {
    open: "",
    pre: "pre-market",
    after: "after hours",
    closed: "closed",
  };
  return t.state ? labels[t.state] : "";
}

/** `+0.8%`, `−1.2%` (a real minus, so the sign lines up with the plus). */
export function percent(value: number): string {
  const rounded = Math.round(value * 10) / 10;
  if (rounded === 0) return "0.0%";
  return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded).toFixed(1)}%`;
}

/** 232.14, 85,809.20: two decimals and grouping, whatever the magnitude. */
export function price(value: number): string {
  return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}
