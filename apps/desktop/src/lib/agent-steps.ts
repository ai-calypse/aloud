import type {
  ActionPreview,
  FinderResults,
  PlannedCall,
  ToolCall,
  ToolResult,
} from "./bindings";
import type { ToolArgs } from "./tool-views";

/**
 * One tool call in a run, with everything the panel shows about it.
 *
 * A list, not one call per run: with a single call and result, the second call
 * inherited the first's result and its confirmation gate never rendered.
 */
export type Step = {
  /** The loop's id for the call; absent on the fast path's single row. */
  id?: number | null;
  name: string;
  /** Once its gate is up, the app as its permission names it: "Notes", not "macOS". */
  app: string;
  /** That app's icon as a data URI; absent draws a neutral glyph. */
  icon?: string | null;
  /** The tool's own argument object; views pick the fields worth showing. */
  args: ToolArgs;
  requiresConfirmation: boolean;
  /** The gate's card, built in Rust from the real arguments. Only a write has one. */
  preview?: ActionPreview | null;
  /** The gate may offer "Always allow"; false for strict and outside-content cards. */
  canAlways?: boolean;
  /** Set by the gate's buttons; undefined while this step's gate is open. */
  confirmed?: boolean;
  /** Kept whole: its `show` is what the finished row's Show button brings back. */
  result?: ToolResult;
  /**
   * A Finder search's results already in hand, so its list draws without
   * searching again — the tuning page's, which has no Mac to search.
   */
  found?: FinderResults;
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
 * Where a call event belongs: the step carrying its id, else — for the fast
 * path, whose events carry none — the last step if it is the same call still
 * waiting for its result.
 */
function stepFor(steps: Step[], call: ToolCall): number {
  if (call.id != null) return steps.findIndex((s) => s.id === call.id);
  const last = steps.length - 1;
  const open = steps[last];
  return open && !open.result && open.name === call.name ? last : -1;
}

/**
 * Folds a tool-call event into the step list. Rust emits two events per write —
 * the start, then the same call with `requiresConfirmation` — so the second
 * updates its step rather than appending a duplicate. A round's calls all
 * start before any finishes, so the step is found by id, not by being last.
 */
export function withCall(steps: Step[], call: ToolCall): Step[] {
  const at = stepFor(steps, call);
  if (at !== -1) {
    const open = steps[at];
    return steps.map((s, i) =>
      i === at
        ? {
            ...open,
            // The gate names the app as its permission does; the start event
            // only guessed from the tool's name.
            app: call.preview ? call.app : open.app,
            icon: call.icon ?? open.icon,
            args: asArgs(call.args),
            requiresConfirmation: open.requiresConfirmation || call.requiresConfirmation,
            // Only the gate event carries one; whichever of the pair lands
            // second must not wipe it.
            preview: call.preview ?? open.preview,
            canAlways: open.canAlways || call.canAlways,
          }
        : s,
    );
  }
  return [
    ...steps,
    {
      id: call.id,
      name: call.name,
      app: call.app,
      icon: call.icon,
      args: asArgs(call.args),
      requiresConfirmation: call.requiresConfirmation,
      preview: call.preview,
      canAlways: call.canAlways,
    },
  ];
}

/**
 * Records the answer to the open gate. A decline marks the step cancelled
 * before Rust's failed result arrives, so the row says what the user did
 * rather than "failed".
 */
export function withDecision(steps: Step[], confirmed: boolean): Step[] {
  const open = pendingStep({ heard: "", steps });
  return steps.map((s) => (s === open ? { ...s, confirmed } : s));
}

/** What a step's row says about how it went. */
export function stepStatus(step: Step, pending: boolean): string {
  if (step.confirmed === false) return "cancelled";
  if (step.result) return step.result.success ? "done" : "failed";
  if (pending) return "needs approval";
  return "running…";
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

/**
 * Attaches a result to its own step: the one with its id, else (the fast path)
 * the last still waiting. A result for a step the panel never saw — a quiet run
 * that surfaced mid-round — is dropped rather than put on another's row.
 */
export function withResult(steps: Step[], result: ToolResult): Step[] {
  let index = -1;
  if (result.id != null) {
    index = steps.findIndex((s) => s.id === result.id && !s.result);
  } else {
    for (let i = steps.length - 1; i >= 0; i--) {
      if (!steps[i].result) {
        index = i;
        break;
      }
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
