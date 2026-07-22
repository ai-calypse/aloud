import type { PlannedCall, ToolCall, ToolResult } from "./bindings";
import type { ToolArgs } from "./tool-views";

/**
 * One tool call in a run, with everything the panel shows about it.
 *
 * A list, not one call per run: with a single call and result, the second call
 * inherited the first's result and its confirmation gate never rendered.
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
  /** Calls this round has not started yet; each leaves as its call starts. */
  planned?: PlannedCall[];
  finished?: string;
  error?: string;
  /** Transient line ("Rate limited — retrying…"); cleared by the next event. */
  status?: string;
  /**
   * The model is waiting on the user for something no tool could give it. The
   * run is suspended in Rust until `answerAgent`, so this is the one state the
   * panel must offer an input for rather than only reporting. `options`
   * non-empty makes it a choice rather than free text.
   */
  question?: { question: string; options: string[] };
};

/** Tool arguments cross as free-form JSON; anything but an object is empty. */
function asArgs(value: unknown): ToolArgs {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as ToolArgs)
    : {};
}

/**
 * Folds a tool-call event into the step list. Rust emits two events per write —
 * the start, then the same call with `requiresConfirmation` — so the second
 * updates the open step rather than appending a duplicate.
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

/**
 * Drops the first queued entry for `name`, and only the first, so a round that
 * calls the same tool twice ticks off one per start rather than both.
 */
export function withoutPlanned(
  planned: PlannedCall[] | undefined,
  name: string,
): PlannedCall[] | undefined {
  if (!planned?.length) return planned;
  const at = planned.findIndex((p) => p.name === name);
  if (at === -1) return planned;
  const left = [...planned.slice(0, at), ...planned.slice(at + 1)];
  return left.length ? left : undefined;
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


/** A run of adjacent steps the panel draws as one row. */
export type StepGroup = {
  /** The last of them — its result is the one the row reports. */
  step: Step;
  /** How many collapsed here; 1 for the overwhelming majority. */
  count: number;
};

/**
 * Collapses adjacent steps that would render identically.
 *
 * A lookup for one person routinely costs three calls — the contacts directory,
 * then the mailbox, then the mailbox again with a different query — and the
 * panel listed each as its own row, so a single "find Charlie" read as four
 * things happening. Adjacent rows with the same rendered text now stack.
 *
 * Keyed on what the row actually shows, not on the tool name: two calls only
 * merge when the user could not have told them apart anyway. That is what stops
 * a failure being absorbed into the success above it, or the step awaiting
 * approval being folded away — their rows read differently, so they never
 * merge. Non-adjacent repeats stay separate; they are a real "went back and
 * looked again", which is worth seeing.
 */
export function groupSteps(steps: Step[], rowKey: (step: Step) => string): StepGroup[] {
  const groups: StepGroup[] = [];
  let previousKey: string | null = null;
  for (const step of steps) {
    const key = rowKey(step);
    const last = groups[groups.length - 1];
    if (last && key === previousKey) {
      last.step = step;
      last.count += 1;
    } else {
      groups.push({ step, count: 1 });
    }
    previousKey = key;
  }
  return groups;
}
