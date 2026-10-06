import type { ChargeState, Thermal } from "@/lib/bindings";
import type { Tone } from "@/components/notch/meters";

/** The Rust floats arrive as `null` when they were NaN; a bar and a number want 0. */
export function num(n: number | null | undefined): number {
  return n != null && Number.isFinite(n) ? n : 0;
}

/** Bytes per second as a short rate: "0 B/s", "812 KB/s", "1.4 MB/s". */
export function formatRate(bytesPerSecond: number | null): string {
  const b = num(bytesPerSecond);
  if (b < 1000) return `${Math.round(b)} B/s`;
  if (b < 1_000_000) return `${Math.round(b / 1000)} KB/s`;
  const mb = b / 1_000_000;
  return `${mb < 100 ? mb.toFixed(1) : Math.round(mb)} MB/s`;
}

/** Gigabytes, one decimal under 100: "11.2 GB", "460 GB". */
export function formatGb(gb: number | null): string {
  const v = num(gb);
  return `${v < 100 ? v.toFixed(1) : Math.round(v)} GB`;
}

/** Minutes as "45m" or "7h 20m". */
export function formatMinutes(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h === 0 ? `${m}m` : `${h}h ${m.toString().padStart(2, "0")}m`;
}

/** The sentence under the battery's lead: what it is doing and for how long. */
export function chargeSentence(state: ChargeState, minutes: number | null): string {
  switch (state) {
    case "discharging":
      return minutes != null ? `${formatMinutes(minutes)} left` : "On battery, estimating time left";
    case "charging":
      return minutes != null ? `Charging, ${formatMinutes(minutes)} to full` : "Charging";
    case "full":
      return "Fully charged";
    case "pluggedIn":
      return "Plugged in, not charging";
  }
}

/** The meter's tone: red and amber when it is about to run out, green while it fills. */
export function batteryTone(percent: number, state: ChargeState): Tone {
  if (state === "charging" || state === "full") return "ok";
  return percent <= 10 ? "danger" : percent <= 20 ? "warn" : "ink";
}

const THERMAL: Record<Thermal, { word: string; tone: Tone }> = {
  nominal: { word: "Normal", tone: "ink" },
  fair: { word: "Warm", tone: "ink" },
  serious: { word: "Hot", tone: "warn" },
  critical: { word: "Critical", tone: "danger" },
};

export function thermalReading(thermal: Thermal): { word: string; tone: Tone } {
  return THERMAL[thermal];
}
