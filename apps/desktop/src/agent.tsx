import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { CheckIcon, CopyIcon, TriangleAlertIcon, XIcon } from "lucide-react";
import { commands, events } from "./lib/bindings";
import {
  pendingStep,
  withCall,
  withResult,
  type Run,
  type Step,
} from "./lib/agent-steps";
import { bodyFor, labelFor } from "./lib/tool-views";
import { usePanelAutosize } from "./lib/autosize";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { MessageResponse } from "@/components/ai-elements/message";
import { Loader } from "@/components/ai-elements/loader";
import {
  Confirmation,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRequest,
  ConfirmationTitle,
} from "@/components/ai-elements/confirmation";
import "./index.css";
import "./panel.css";

function stepStatus(step: Step, pending: boolean): string {
  if (step.result) return step.result.success ? "done" : "failed";
  if (step.confirmed === false) return "cancelled";
  if (pending) return "needs approval";
  return "running…";
}

function StepIcon({ step, pending }: { step: Step; pending: boolean }) {
  if (step.result)
    return step.result.success ? (
      <CheckIcon className="size-3.5 shrink-0 text-muted-foreground" />
    ) : (
      <XIcon className="size-3.5 shrink-0 text-destructive" />
    );
  if (step.confirmed === false)
    return <XIcon className="size-3.5 shrink-0 text-muted-foreground" />;
  if (pending)
    return <TriangleAlertIcon className="size-3.5 shrink-0 text-[var(--warning)]" />;
  return <Loader />;
}

/**
 * The agent panel, in the same card chrome as the ask panel. Shows what was
 * heard, surfaces the tool call it resolved to, asks to confirm a write
 * before it fires, then reports the result.
 */
function Agent() {
  const [run, setRun] = useState<Run>({ heard: "", steps: [] });
  const [copied, setCopied] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);

  usePanelAutosize(cardRef);

  useEffect(() => {
    const unlisten = events.agent.listen(({ payload }) => {
      switch (payload.kind) {
        case "runStarted":
          // A new run resets the panel; it is reused, not recreated.
          setRun({ heard: payload.value, steps: [] });
          setCopied(false);
          break;
        case "toolCall":
          setRun((r) => ({
            ...r,
            steps: withCall(r.steps, payload.value),
            status: undefined,
          }));
          break;
        case "toolResult":
          setRun((r) => ({
            ...r,
            steps: withResult(r.steps, payload.value),
            status: undefined,
          }));
          break;
        case "status":
          setRun((r) => ({ ...r, status: payload.value }));
          break;
        case "runFinished":
          setRun((r) => ({ ...r, finished: payload.value, status: undefined }));
          break;
        case "runError":
          setRun((r) => ({ ...r, error: payload.value, status: undefined }));
          break;
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Escape cancels: a pending confirmation reads as No, so nothing fires.
      if (e.key === "Escape") commands.closeAgent();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const confirm = (approved: boolean) => {
    setRun((r) => ({
      ...r,
      steps: r.steps.map((s) =>
        s.requiresConfirmation && s.confirmed === undefined && !s.result
          ? { ...s, confirmed: approved }
          : s,
      ),
    }));
    commands.confirmAgent(approved);
  };

  const pending = pendingStep(run);
  const copyText = run.finished || "";

  return (
    <Card ref={cardRef} className="panel gap-3 rounded-2xl border-white/10 py-4 shadow-none">
      <CardHeader className="px-4">
        <p
          className="m-0 line-clamp-2 text-[13px] font-medium text-muted-foreground"
          data-tauri-drag-region
        >
          {run.heard}
        </p>
      </CardHeader>
      <Separator className="bg-white/10" />

      <CardContent className="panel__answer flex flex-col gap-3 px-4 text-[15px] leading-normal">
        {/* Before any tool call resolves, we're still parsing the transcript. */}
        {run.steps.length === 0 && !run.error && !run.status && (
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader />
            Understanding…
          </div>
        )}

        {/* One line per tool: what is running and how it went. The arguments
            are deliberately not shown here — they are a wall of JSON, and the
            only moment they matter is when approving a write, below. */}
        {run.steps.length > 0 && (
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {run.steps.map((step, i) => (
              <li key={`${step.name}-${i}`} className="flex flex-col gap-1.5">
                <div className="flex items-center gap-2 text-[13px]">
                  <StepIcon step={step} pending={step === pending} />
                  <span className="min-w-0 flex-1 truncate">
                    {labelFor(step.name, step.args)}
                  </span>
                  <span className="shrink-0 text-[11px] text-muted-foreground">
                    {stepStatus(step, step === pending)}
                  </span>
                </div>
                {/* Only what this tool is worth showing. A step with no
                    registered view renders its label and nothing else. */}
                {step !== pending && (
                  <div className="pl-[22px]">{bodyFor(step.name, step.args)}</div>
                )}
              </li>
            ))}
          </ul>
        )}

        {pending && (
          <Confirmation approval={{ id: pending.name }} state="approval-requested">
            <ConfirmationTitle>
              {labelFor(pending.name, pending.args)} — this writes to{" "}
              {pending.app}.
            </ConfirmationTitle>
            <ConfirmationRequest>
              {/* Approving a write without seeing what it will do is not a
                  gate, so the body shows here even though the step list is
                  otherwise quiet. */}
              <div className="mb-3">{bodyFor(pending.name, pending.args)}</div>
              <ConfirmationActions>
                <ConfirmationAction variant="outline" onClick={() => confirm(false)}>
                  Cancel
                </ConfirmationAction>
                <ConfirmationAction autoFocus onClick={() => confirm(true)}>
                  Confirm
                </ConfirmationAction>
              </ConfirmationActions>
            </ConfirmationRequest>
          </Confirmation>
        )}

        {run.status && (
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader />
            {run.status}
          </div>
        )}

        {/* The run's closing answer — the whole point of the run. */}
        {run.finished && <MessageResponse>{run.finished}</MessageResponse>}

        {run.error && <p className="m-0 text-destructive">{run.error}</p>}
      </CardContent>

      <CardFooter className="gap-2.5 px-4">
        <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-muted-foreground/70">
          {run.steps.length > 0
            ? `· ${labelFor(
                run.steps[run.steps.length - 1].name,
                run.steps[run.steps.length - 1].args,
              )}`
            : ""}
        </span>
        {copyText && (
          <Button
            variant="secondary"
            size="sm"
            className="rounded-full"
            onClick={() =>
              navigator.clipboard.writeText(copyText).then(() => setCopied(true))
            }
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? "Copied" : "Copy"}
          </Button>
        )}
        <Button
          variant="secondary"
          size="icon-sm"
          className="rounded-full"
          aria-label="Close"
          onClick={() => commands.closeAgent()}
        >
          <XIcon />
        </Button>
      </CardFooter>
    </Card>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Agent />
  </React.StrictMode>,
);
