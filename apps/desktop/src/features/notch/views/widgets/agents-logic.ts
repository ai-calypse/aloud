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

/** "2 h 14 m", "36 m", "<1 m": time saved at the grain a glance wants. Whole minutes, hours left bare when exact. */
export function savedDuration(seconds: number | null | undefined): string {
  const value = Number.isFinite(seconds) ? (seconds as number) : 0;
  if (value <= 0) return "0 m";
  if (value < 30) return "<1 m";
  const minutes = Math.round(value / 60);
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h === 0 ? `${m} m` : m === 0 ? `${h} h` : `${h} h ${m} m`;
}

/** The share of a total that one part is, 0…1, with an empty total drawing as none. */
export function savedShare(part: number | null | undefined, total: number | null | undefined): number {
  return total && part && total > 0 ? Math.min(1, Math.max(0, part / total)) : 0;
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

/**
 * A long markdown reply as the sentence or two it opens with, for a tile that
 * is one glance wide. Headings, fences, list markers and emphasis are dropped:
 * what is left is prose, cut on a word and ended with an ellipsis.
 */
export function plainSummary(markdown: string, max = 200): string {
  let fenced = false;
  const prose: string[] = [];
  for (const raw of markdown.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("```")) {
      fenced = !fenced;
      continue;
    }
    if (fenced || line === "" || line.startsWith("#")) continue;
    prose.push(
      line
        .replace(/^([-*+]|\d+\.)\s+/, "")
        .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/[*_`]/g, ""),
    );
  }
  const flat = prose.join(" ").replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  const cut = flat.slice(0, max);
  return `${cut.slice(0, Math.max(cut.lastIndexOf(" "), max / 2)).replace(/[\s,.;:]+$/, "")}…`;
}
