import type { ToolCall, ToolResult } from "./bindings";
import type { ToolArgs } from "./tool-views";

/**
 * One tool call in a run, with everything the panel shows about it.
 *
 * A run can make several calls — "send this and draft a reply" is two — and
 * they resolve one at a time. The panel used to keep a single call and a
 * single result for the whole run, which meant the second call inherited the
 * first one's result and its confirmation gate never rendered: the write sat
 * waiting on a button that was never drawn. Steps are a list for that reason,
 * not for looks.
 */
export type Step = {
  name: string;
  app: string;
  /** The tool's own argument object; views pick the fields worth showing. */
  args: ToolArgs;
  requiresConfirmation: boolean;
  /** Set by Confirm/Cancel; undefined while this step's gate is open. */
  confirmed?: boolean;
  result?: ToolResult;
};

export type Run = {
  heard: string;
  steps: Step[];
  finished?: string;
  error?: string;
  /** Transient line ("Rate limited — retrying…"); cleared by the next event. */
  status?: string;
};

/** Tool arguments cross as free-form JSON; anything but an object is empty. */
function asArgs(value: unknown): ToolArgs {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as ToolArgs)
    : {};
}

/**
 * Folds a tool-call event into the step list.
 *
 * Rust emits two events per call: the start, then — for a write — the same
 * call again with `requiresConfirmation`. Both name the same tool, so the
 * second updates the open step rather than appending a duplicate.
 */
export function withCall(steps: Step[], call: ToolCall): Step[] {
  const open = steps.length > 0 ? steps[steps.length - 1] : undefined;
  if (open && open.name === call.name && !open.result) {
    return [
      ...steps.slice(0, -1),
      {
        ...open,
        args: asArgs(call.args),
        requiresConfirmation: open.requiresConfirmation || call.requiresConfirmation,
      },
    ];
  }
  return [
    ...steps,
    {
      name: call.name,
      app: call.app,
      args: asArgs(call.args),
      requiresConfirmation: call.requiresConfirmation,
    },
  ];
}

/** Attaches a result to the last step still waiting for one. */
export function withResult(steps: Step[], result: ToolResult): Step[] {
  let index = -1;
  for (let i = steps.length - 1; i >= 0; i--) {
    if (!steps[i].result) {
      index = i;
      break;
    }
  }
  if (index === -1) return steps;
  return steps.map((s, i) => (i === index ? { ...s, result } : s));
}

/** The step whose gate is open, if any. Only one can be at a time. */
export function pendingStep(run: Run): Step | undefined {
  return run.steps.find(
    (s) => s.requiresConfirmation && s.confirmed === undefined && !s.result,
  );
}

