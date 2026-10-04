import React, { useEffect, useState, type CSSProperties } from "react";
import {
  CheckIcon,
  CircleDashedIcon,
  LoaderCircleIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import {
  commands,
  type FinderResults,
  type GateDecision,
  type ShowTarget,
} from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import { PreviewCard } from "@/components/preview/cards";
import { CardEditsContext } from "@/components/preview/kit";
import {
  groupSteps,
  pendingStep,
  stepStatus,
  type Run,
  type Step,
} from "@/lib/agent-steps";
import { gateFor, labelFor, text } from "@/lib/tool-views";
import { Panel } from "./Panel";

/**
 * An agent run, in the notch panel: what was heard, the tools it resolved to,
 * the approval a write has to pass, and the closing answer.
 */
export function AgentRun({
  run,
  bodyRef,
  onAddTab,
  onDecide,
}: {
  run: Run;
  bodyRef: React.RefObject<HTMLDivElement | null>;
  onAddTab?: () => void;
  /** The open gate was answered: `confirmed` false is a decline. */
  onDecide?: (confirmed: boolean) => void;
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
                {step.result?.tidied && <UndoButton folder={step.result.tidied} />}
              </span>
              {/* Why a saved grant refused it — kept on the row, where the
                  status line it used to be was wiped by this very result. */}
              {step.result?.note && <p className="agent__reason">{step.result.note}</p>}
              {/* What a write did, as a compact echo of the card it was
                  approved from — and never for the step awaiting a decision:
                  the gate below shows that one, and rendering both put the
                  same draft on screen twice. */}
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
                  step.preview && (
                    <div className="agent__body">
                      <PreviewCard preview={step.preview} compact />
                    </div>
                  )
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

      {/* Keyed on the card's revision, so a second gate — or the same card
          redrawn by "make it …" — starts from its own draft and countdown. */}
      {pending && (
        <ConfirmGate
          key={`${pending.name}-${pending.preview?.revision ?? run.steps.length}`}
          step={pending}
          onDecide={onDecide}
        />
      )}

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
 * The gate a write passes before it runs: the card Rust drew from its real
 * arguments (`PreviewCard`), so no write is ever approved from a title alone.
 * Fields Rust marked editable are edited in place, and an exact-words message
 * counts itself down — sent at the deadline unless the user reaches for it.
 *
 * Every answer carries the card's revision, so one meant for this card can
 * never answer the next.
 */
function ConfirmGate({
  step,
  onDecide,
}: {
  step: Step;
  onDecide?: (confirmed: boolean) => void;
}) {
  const preview = step.preview;
  const revision = preview?.revision ?? 0;
  const autoMs = preview?.autoMs ?? null;
  // Strict cards take a click: no Enter, focus on Don't allow, no Always.
  const strict = preview?.tier === "strict";
  // Only what the user changed, by pointer. Rust writes it into the real
  // arguments; anything untouched keeps the value the model chose.
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [held, setHeld] = useState(false);
  const counting = autoMs !== null && !held;
  // An editable field the model left empty is the one to fill, so it takes
  // focus rather than the allow button.
  const missing = [...(preview?.fields ?? []), ...(preview?.more ?? [])].some(
    (f) => f.editable && f.value === "",
  );

  // Armed from here, once the card is on screen, so the bar the user watches
  // and the deadline in Rust start together.
  useEffect(() => {
    if (autoMs !== null) void commands.armAgent(revision);
  }, [autoMs, revision]);

  const hold = () => {
    if (!counting) return;
    setHeld(true);
    void commands.holdAgent(revision);
  };

  const decide = (decision: GateDecision) => {
    const approved = decision !== "decline";
    const edits = approved && Object.keys(draft).length > 0 ? draft : null;
    void commands.confirmAgent(decision, edits, revision);
    onDecide?.(approved);
  };

  // ApprovalCard's keys: Enter allows once, Escape declines. Capture phase so
  // the panel's own Escape does not also fire.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        decide("decline");
        return;
      }
      if (e.key !== "Enter" || strict) return;
      // A focused button answers for itself; a message keeps Enter for new
      // lines, and ⌘↩ sends it.
      if (e.target instanceof HTMLButtonElement) return;
      if (e.target instanceof HTMLTextAreaElement && !e.metaKey) return;
      e.preventDefault();
      decide("allowOnce");
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div
      className="agent__confirm"
      role="group"
      data-danger={strict || undefined}
      // Reaching for the card stops the countdown: whoever moves toward it
      // means to read it, or to stop it.
      onPointerMove={hold}
    >
      <p
        className={strict ? "agent__confirm-title agent__confirm-title--danger" : "agent__confirm-title"}
      >
        {gateFor(step.name, step.args) ||
          preview?.title ||
          `${labelFor(step.name, step.args)} — this writes to ${step.app}.`}
      </p>
      {preview?.scope && <p className="agent__scope">{preview.scope}</p>}
      {preview && (
        <CardEditsContext.Provider
          value={{
            draft,
            edit: (path, value) => {
              hold();
              setDraft((current) => ({ ...current, [path]: value }));
            },
            hold,
          }}
        >
          <PreviewCard preview={preview} />
        </CardEditsContext.Provider>
      )}
      {autoMs !== null && (
        <div
          className="countdown motion-countdown"
          data-held={held || undefined}
          aria-hidden
          style={{ "--auto-ms": `${autoMs}ms` } as CSSProperties}
        />
      )}
      <div className="perm__acts">
        <button
          className="btn btn--allow"
          autoFocus={!strict && !missing}
          onClick={() => decide("allowOnce")}
        >
          {preview?.verb || "Allow"} once
          {!strict && <span className="btn__k">⏎</span>}
        </button>
        {step.canAlways && !strict && (
          <button className="btn btn--always" onClick={() => decide("allowAlways")}>
            Always allow
          </button>
        )}
        <button
          className="btn btn--deny"
          autoFocus={strict}
          onClick={() => decide("decline")}
        >
          {/* On a card that counts down, declining is taking it back before it
              goes. Still "Undo" once held: reaching for it is what holds it,
              and a label changing under the pointer is a misclick. */}
          {autoMs !== null ? "Undo" : "Don't allow"} <span className="btn__k">esc</span>
        </button>
      </div>
    </div>
  );
}

/**
 * Puts a tidied folder back as it was. The only write with a reverse already
 * built, so the only row with an Undo.
 */
function UndoButton({ folder }: { folder: string }) {
  const [undone, setUndone] = useState(false);
  return (
    <Button
      variant="plain"
      size="text"
      disabled={undone}
      onClick={() =>
        void commands.undoTidy(folder).then((res) => setUndone(res.status === "ok"))
      }
    >
      {undone ? "Undone" : "Undo"}
    </Button>
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
  // Before the result: a declined write's result is a failure, but the user
  // said no — a neutral mark, not the red one.
  if (step.confirmed === false) return <XIcon className="agent__icon" size={14} />;
  if (step.result) {
    return step.result.success ? (
      <CheckIcon className="agent__icon" size={14} />
    ) : (
      <XIcon className="agent__icon agent__icon--bad" size={14} />
    );
  }
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
