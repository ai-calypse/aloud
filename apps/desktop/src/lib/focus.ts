import type { TimerState } from "@/lib/bindings";

/**
 * The timer's face, derived from the wall clock Rust handed over (`at`,
 * `heldMs`). The webview never counts: sleeping, a slow render or a missed
 * tick cannot make it wrong, only late. Mirrors `TimerState::value_ms`.
 */
export function timerValueMs(s: TimerState, now: number): number {
  if (s.status !== "running") return s.heldMs;
  if (s.mode === "stopwatch") return Math.max(0, now - s.at);
  return Math.min(s.totalMs, Math.max(0, s.at - now));
}

/** `18:42`, `1:05:00`. A countdown rounds up, so 0:00 only shows when it is over. */
export function clock(ms: number, countdown: boolean): string {
  const total = Math.max(0, ms);
  const secs = countdown ? Math.ceil(total / 1000) : Math.floor(total / 1000);
  const h = Math.floor(secs / 3600);
  const m = Math.floor(secs / 60) % 60;
  const s = secs % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

/**
 * Milliseconds until the face next changes, so the view re-renders exactly
 * when a digit does instead of sampling twice a second. The 4ms is margin:
 * a timeout that fires a hair early would show the old second again.
 */
export function nextTickIn(s: TimerState, now: number): number {
  const phase = s.mode === "stopwatch" ? now - s.at : s.at - now;
  const mod = ((phase % 1000) + 1000) % 1000;
  const wait = s.mode === "stopwatch" ? 1000 - mod : mod === 0 ? 1000 : mod;
  return wait + 4;
}

/** What the lead's sub-line says: where in the session the timer is. */
export function timerTitle(s: TimerState): string {
  if (s.mode === "stopwatch") return "Stopwatch";
  if (s.phase === "focus") return `Focus ${s.round} of ${s.rounds}`;
  return s.phase === "shortBreak" ? "Short break" : "Long break";
}
