import type { AgentSession } from "@/lib/bindings";
import { pendingStep, type Run, type Step } from "@/lib/agent-steps";
import { truncate } from "@/lib/format";
import type { WidgetRole } from "@/components/notch/widgets";

/**
 * What the Agents widgets say, worked out from the data the notch reads.
 * Apart from the components so the words, which can silently be wrong, are
 * testable; the components only place them.
 */

/** The plan-usage thresholds (70 amber, 90 red), as words. */
export function usageWarning(pct: number): string | null {
  return pct >= 90 ? "Nearly full" : pct >= 70 ? "Getting full" : null;
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
