import React, { useEffect, useState } from "react";
import {
  CheckIcon,
  CircleDashedIcon,
  LoaderCircleIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import { commands, type FinderResults, type ShowTarget } from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import { FallbackCard } from "@/components/preview/kit";
import {
  groupSteps,
  pendingStep,
  type Run,
  type Step,
} from "@/lib/agent-steps";
import {
  askFor,
  bodyFor,
  editsFor,
  gateFor,
  labelFor,
  text,
  type ToolArgs,
} from "@/lib/tool-views";
import { Panel } from "./Panel";

/**
 * An agent run, in the notch panel: what was heard, the tools it resolved to,
 * the approval a write has to pass, and the closing answer.
 */
export function AgentRun({
  run,
  bodyRef,
  onAddTab,
}: {
  run: Run;
  bodyRef: React.RefObject<HTMLDivElement | null>;
  onAddTab?: () => void;
}) {
  const pending = pendingStep(run);

  return (
    <Panel
      title={run.heard}
      bodyRef={bodyRef}
      // Nothing to copy until the run has said something.
      copyText={run.finished || undefined}
      onClose={() => void commands.closeAgent()}
      onAddTab={onAddTab}
      footer={
        <span className="agent__trail">
          {run.steps.length > 0
            ? labelFor(
                run.steps[run.steps.length - 1].name,
                run.steps[run.steps.length - 1].args,
              )
            : ""}
        </span>
      }
    >
      {/* Before any tool call resolves, we're still parsing the transcript. A
          plain answer finishes with no steps at all, so `finished` ends it too —
          without it the loader sat above the answer. */}
      {run.steps.length === 0 && !run.error && !run.status && !run.finished && (
        <p className="agent__status">
          <LoaderCircleIcon className="size-4 animate-spin" />
          {/* `as="span"`: Shimmer renders a <p> by default, and a <p> inside a
              <p> is invalid — React closes the outer one early and the layout
              silently breaks. */}
          <Shimmer as="span">Understanding…</Shimmer>
        </p>
      )}

      {/* One line per tool: what is running and how it went. The arguments are
          deliberately not shown here — they are a wall of JSON, and the only
          moment they matter is when approving a write, below. */}
      {(run.steps.length > 0 || (run.planned?.length ?? 0) > 0) && (
        <ul className="agent__steps">
          {/* Keyed on the row's own text, so only steps the user could not have
              told apart collapse — see `groupSteps`. */}
          {groupSteps(run.steps, (s) => rowKey(s, s === pending)).map(
            ({ step, count }, i) => (
            <li key={`${step.name}-${i}`}>
              <span className="agent__step">
                <StepIcon step={step} pending={step === pending} />
                <span className="agent__name">{labelFor(step.name, step.args)}</span>
                {count > 1 && <span className="agent__count">×{count}</span>}
                <span className="agent__state">
                  {stepStatus(step, step === pending)}
                </span>
                {step.result?.show && <ShowButton show={step.result.show} />}
              </span>
              {/* Only what this tool is worth showing, and never for the step
                  awaiting a decision — the gate below shows that one, and
                  rendering both put the same draft on screen twice. */}
              {step !== pending &&
                (step.name === "ALOUD_FINDER_SEARCH" ? (
                  // The search wall renders its own thumbnails once the results
                  // land, so it replaces the static body rather than sitting
                  // beside it.
                  <FinderGrid
                    query={text(step.args, "query")}
                    kind={text(step.args, "kind")}
                  />
                ) : (
                  <span className="agent__body">{bodyFor(step.name, step.args)}</span>
                ))}
            </li>
            ),
          )}
          {/* What has not happened yet, in the same list so the run reads in
              order rather than as two stacked sections. */}
          {run.planned?.map((next, i) => (
            <li key={`planned-${next.name}-${i}`} className="agent__step--planned">
              <span className="agent__step">
                <CircleDashedIcon className="agent__icon" size={14} />
                {/* No arguments yet — they arrive with the call. */}
                <span className="agent__name">{labelFor(next.name, {})}</span>
                <span className="agent__state">queued</span>
              </span>
            </li>
          ))}
        </ul>
      )}

      {/* Keyed on the step, so a second gate in one run starts from its own
          arguments instead of inheriting the first one's draft. */}
      {pending && <ConfirmGate key={`${pending.name}-${run.steps.length}`} step={pending} />}

      {/* Keyed on the question so a second one clears the first one's answer. */}
      {run.question && !pending && (
        <AskGate key={run.question.question} asked={run.question} />
      )}

      {run.status && (
        <p className="agent__status">
          <LoaderCircleIcon className="size-4 animate-spin" />
          <Shimmer as="span">{run.status}</Shimmer>
        </p>
      )}

      {run.finished && <MessageResponse>{run.finished}</MessageResponse>}
      {run.error && <p className="ask__error">{run.error}</p>}
    </Panel>
  );
}

/**
 * The model asking the user for something no tool could give it.
 *
 * The run is suspended in Rust for as long as this is on screen, so it is the
 * one place the panel takes input that is not a decision. Dismissing answers
 * "nothing", which the loop reports as unanswered — the model then closes the
 * run rather than retrying into the same wall.
 */
/**
 * The Finder search result wall — Option 1, a 4-up thumbnail grid under a
 * "find my …" step. The model runs the search tool itself and speaks the count;
 * this fetches the same query again for thumbnails, so the panel shows the files
 * without the tool payload being piped back through the loop.
 *
 * Fetches once per (query, kind). A click reveals the file in Finder. Renders
 * nothing until results land — the panel's own height animation covers the gap,
 * and an empty grid flashing before the fill would fight it.
 */
function FinderGrid({ query, kind }: { query: string; kind: string }) {
  const [results, setResults] = useState<FinderResults | null>(null);

  useEffect(() => {
    let live = true;
    void commands.finderSearch(query, kind || "any").then((res) => {
      // The command returns Result; ignore the error arm — a failed search
      // already surfaced through the tool result the model speaks.
      if (live && res.status === "ok") setResults(res.data);
    });
    return () => {
      live = false;
    };
  }, [query, kind]);

  if (!results || results.files.length === 0) return null;

  const noun = kind === "image" ? "image" : "file";
  return (
    <div className="finder">
      <p className="finder-grid__count">
        {results.count} {noun}
        {results.count === 1 ? "" : "s"}
      </p>
      <div className="finder-grid" role="list">
        {results.files.map((file, i) => (
          <button
            key={file.path}
            className="finder-tile"
            role="listitem"
            title={file.name}
            style={{ ["--i" as string]: i }}
            // Arm the native drag on mousedown: a real system file drag can only
            // start from the live mouse event, which a JS click cannot supply,
            // so Rust stashes the path and the native monitor drags it. See
            // crates/drag. Cleared on mouseup so a plain click stays a click.
            onMouseDown={() => void commands.armFileDrag(file.path)}
            onMouseUp={() => void commands.disarmFileDrag()}
            onClick={() => void commands.finderReveal(file.path)}
          >
            {file.thumbnail ? (
              // draggable=false so the img's own image-bytes drag never
              // pre-empts the tile's file drag — we want a file, not a picture.
              <img src={file.thumbnail} alt={file.name} draggable={false} />
            ) : (
              <span className="finder-tile__name">{file.name}</span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

function AskGate({ asked }: { asked: { question: string; options: string[] } }) {
  const [typed, setTyped] = useState("");
  const answer = (text: string | null) => void commands.answerAgent(text);

  // A choice, not a blank. Offered whenever the model knows the candidates —
  // which of three Sams, which calendar — because picking cannot be misspelled
  // and a spoken app should never make someone type an address they can see.
  if (asked.options.length > 0) {
    return (
      <div className="agent__confirm" role="group">
        <p className="agent__confirm-title">{asked.question}</p>
        <div className="agent__options">
          {asked.options.map((option) => (
            <button
              className="agent__option"
              key={option}
              onClick={() => answer(option)}
            >
              {option}
            </button>
          ))}
        </div>
        <div className="agent__actions">
          <button className="agent__action" onClick={() => answer(null)}>
            None of these
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="agent__confirm" role="group">
      <p className="agent__confirm-title">{asked.question}</p>
      <form
        className="agent__ask"
        onSubmit={(e) => {
          e.preventDefault();
          if (typed.trim()) answer(typed.trim());
        }}
      >
        <input
          className="ask__input"
          autoFocus
          value={typed}
          placeholder="Type your answer…"
          onChange={(e) => setTyped(e.target.value)}
        />
        <button className="agent__action" type="button" onClick={() => answer(null)}>
          Skip
        </button>
        <button
          className="agent__action agent__action--go"
          type="submit"
          disabled={!typed.trim()}
        >
          Send
        </button>
      </form>
    </div>
  );
}

/**
 * The gate a write passes before it runs.
 *
 * Most tools show what they are about to do and take a yes. A few declare
 * editable fields (`edits` in tool-views) and show inputs instead — the agent
 * reaches the contact gate precisely because it could not find an address, so
 * refusing is the only other answer it could give.
 *
 * A tool with no hand-drawn view shows the card Rust built from its schema, so
 * no write is ever approved from a title alone.
 */
function ConfirmGate({ step }: { step: Step }) {
  const fields = editsFor(step.name);
  const [draft, setDraft] = useState<ToolArgs>(step.args);
  // A form is a demand, and the agent reaches this one on its own initiative.
  // Ask first; the fields only appear for someone who said yes.
  const [filling, setFilling] = useState(false);
  const asking = fields.length > 0 && !filling;
  const body = bodyFor(step.name, draft);
  const card = fields.length === 0 && body == null ? step.preview : null;

  // Only what the user could have changed. Rust folds it back into the real
  // nested arguments; anything untouched keeps the value the model chose.
  const confirm = (approved: boolean) =>
    void commands.confirmAgent(
      approved,
      approved && fields.length > 0 ? asStrings(draft) : null,
    );

  return (
    <div className="agent__confirm" role="group">
      <p className="agent__confirm-title">
        {asking && askFor(step.name, draft)
          ? askFor(step.name, draft)
          : gateFor(step.name, draft) ||
            card?.title ||
            `${labelFor(step.name, draft)} — this writes to ${step.app}.`}
      </p>
      {/* Approving a write without seeing what it will do is not a gate, so the
          body shows here even though the step list is otherwise quiet. */}
      {asking ? null : fields.length > 0 ? (
        <div className="agent__fields">
          {fields.map((field) => (
            <label className="agent__field" key={field.key}>
              <span className="agent__field-label">{field.label}</span>
              <input
                className="ask__input"
                value={field.get(draft)}
                placeholder={field.placeholder}
                // The first empty field is the one the agent could not fill.
                autoFocus={!field.get(draft)}
                onChange={(e) =>
                  setDraft((current) => field.set(current, e.target.value))
                }
              />
            </label>
          ))}
        </div>
      ) : card ? (
        <FallbackCard preview={card} />
      ) : (
        <div className="agent__body">{body}</div>
      )}
      <div className="agent__actions">
        <button className="agent__action" onClick={() => confirm(false)}>
          {asking ? "No thanks" : "Cancel"}
        </button>
        {asking ? (
          <button
            className="agent__action agent__action--go"
            autoFocus
            onClick={() => setFilling(true)}
          >
            Save contact
          </button>
        ) : (
          <button
            className="agent__action agent__action--go"
            autoFocus={fields.length === 0}
            onClick={() => confirm(true)}
          >
            {card?.verb || "Confirm"}
          </button>
        )}
      </div>
    </div>
  );
}

/** The flat string map the confirm command takes; see `display_args` in agent.rs. */
function asStrings(args: ToolArgs): Record<string, string> {
  return Object.fromEntries(
    Object.entries(args).map(([key, value]) => [
      key,
      typeof value === "string" ? value : JSON.stringify(value),
    ]),
  );
}

/**
 * Brings back what a finished step made — the note, the saved file, the opened
 * folder. The tools that made it stayed in the background, so this is the way
 * to go and look.
 */
function ShowButton({ show }: { show: ShowTarget }) {
  return (
    <Button
      variant="plain"
      size="text"
      onClick={() => void commands.showResult(show.app, show.target)}
    >
      Show
    </Button>
  );
}

function StepIcon({ step, pending }: { step: Step; pending: boolean }) {
  if (step.result) {
    return step.result.success ? (
      <CheckIcon className="agent__icon" size={14} />
    ) : (
      <XIcon className="agent__icon agent__icon--bad" size={14} />
    );
  }
  if (step.confirmed === false) return <XIcon className="agent__icon" size={14} />;
  if (pending)
    return <TriangleAlertIcon className="agent__icon agent__icon--warn" size={14} />;
  return <LoaderCircleIcon className="agent__icon animate-spin" size={14} />;
}

/**
 * What a step's row says, as one string. Two adjacent steps sharing it render
 * identically, which is exactly when collapsing them hides nothing.
 */
function rowKey(step: Step, pending: boolean): string {
  return `${labelFor(step.name, step.args)}|${stepStatus(step, pending)}`;
}

function stepStatus(step: Step, pending: boolean): string {
  if (step.result) return step.result.success ? "done" : "failed";
  if (step.confirmed === false) return "cancelled";
  if (pending) return "needs approval";
  return "running…";
}
