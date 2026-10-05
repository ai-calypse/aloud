import type { AgentSession, Quota } from "@/lib/bindings";
import { pendingStep, type Run, type Step } from "@/lib/agent-steps";
import { resetsIn, truncate } from "@/lib/format";
import type { WidgetRole } from "@/components/notch/widgets";
import { percent, shownWindows } from "@/features/overlay/QuotaDashboard";

/**
 * What the Agents widgets say, worked out from the data the calm views read.
 * Apart from the components so the words, which can silently be wrong, are
 * testable; the components only place them.
 */

/** Screen Time's three chart colours, one per plan window and always the same one. */
const SHORT: Record<string, string> = { "5-hour window": "5-hour", "Week · Opus": "Opus" };

const WINDOW_ROLE: Record<string, WidgetRole> = {
  "5-hour window": "blue",
  Week: "cyan",
  "Week · Opus": "orange",
};

export type UsageBar = {
  label: string;
  /** Four columns share 310pt, so the column heading is the short name. */
  short: string;
  /** Whole percent, what the words say. */
  percent: number;
  /** 0…1, what the bar and the meter draw. */
  value: number;
  role: WidgetRole;
  resets: string | null;
  /** Said beside the number once a window is close to its cap; colour alone never says it. */
  warning: string | null;
};

/** The calm view's thresholds (70 amber, 90 red), as words. */
export function usageWarning(pct: number): string | null {
  return pct >= 90 ? "Nearly full" : pct >= 70 ? "Getting full" : null;
}

export function usageBars(quota: Quota | null, now = Date.now()): UsageBar[] {
  return shownWindows(quota).map(([label, w]) => {
    const pct = Math.round(percent(w));
    const left = w.resetsAt ? resetsIn(w.resetsAt, now) : "";
    return {
      label,
      short: SHORT[label] ?? label,
      percent: pct,
      value: pct / 100,
      role: WINDOW_ROLE[label] ?? "blue",
      resets: left ? `Resets in ${left}` : null,
      warning: usageWarning(pct),
    };
  });
}

/** Past-the-plan spend, only when there is some, as the calm view's Extra usage row. */
export function extraUsage(quota: Quota | null): number | null {
  const x = quota?.extraUsagePercent;
  return x != null && x > 0 ? Math.round(x) : null;
}

/** One coding session as a tile says it: a word for its state, then what it is doing. */
export function sessionState(
  session: AgentSession,
  asking: boolean,
): { word: string; detail: string; role: WidgetRole | undefined } {
  const word = asking
    ? "Wants permission"
    : session.tool
      ? "Running"
      : session.busy
        ? "Thinking"
        : "Idle";
  const detail =
    session.tool ??
    (session.prompt ? `You: ${truncate(session.prompt, 90)}` : truncate(session.title ?? "", 90));
  return { word, detail, role: asking ? "orange" : session.busy || session.tool ? "agent" : undefined };
}

/** The label under a session's tile: the folder it works in, else the agent. */
export function sessionLabel(session: AgentSession, names: Record<string, string>): string {
  const folder = session.cwd?.split(/[/\\]/).filter(Boolean).pop();
  return truncate(folder ?? names[session.agent] ?? session.agent, 26);
}

export type RunSummary = {
  done: number;
  total: number;
  word: "Failed" | "Asking you" | "Needs your yes" | "Done" | "Working";
  role: WidgetRole;
};

/** Where a voice command is: steps finished of steps known (the queued ones count), and a word. */
export function runSummary(run: Run): RunSummary {
  const pending = pendingStep(run);
  const total = run.steps.length + (run.planned?.length ?? 0);
  const done = run.steps.filter((s) => s.result || s.confirmed === false).length;
  if (run.error) return { done, total, word: "Failed", role: "move" };
  if (pending) return { done, total, word: "Needs your yes", role: "orange" };
  if (run.question) return { done, total, word: "Asking you", role: "orange" };
  if (run.finished) return { done, total, word: "Done", role: "ok" };
  return { done, total, word: "Working", role: "agent" };
}

/** A step's accent: a failure and a waiting gate are told apart by hue and, always, by their word. */
export function stepRole(step: Step, pending: boolean): WidgetRole | undefined {
  if (step.confirmed === false) return undefined;
  if (step.result) return step.result.success ? undefined : "move";
  return pending ? "orange" : undefined;
}
