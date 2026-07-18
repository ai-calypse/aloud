import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import type { ToolUIPart } from "ai";
import { CheckIcon, CopyIcon, XIcon } from "lucide-react";
import { commands, events, type ToolCall, type ToolResult } from "./lib/bindings";
import { usePanelAutosize } from "./lib/autosize";
import { Card, CardContent, CardFooter, CardHeader } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { MessageResponse } from "@/components/ai-elements/message";
import { Loader } from "@/components/ai-elements/loader";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolInput,
  ToolOutput,
} from "@/components/ai-elements/tool";
import {
  Confirmation,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRequest,
  ConfirmationTitle,
} from "@/components/ai-elements/confirmation";
import "./index.css";
import "./panel.css";

/**
 * One AG-UI-style run: what was heard, the tool call it resolved to (if any),
 * that call's result, and the run's closing message or error. The confirm gate
 * lives on the tool call — a write never runs until "Confirm" is pressed here.
 */
type Run = {
  heard: string;
  call?: ToolCall;
  result?: ToolResult;
  /** Set by the Confirm/Cancel buttons; undefined while the gate is open. */
  confirmed?: boolean;
  finished?: string;
  error?: string;
  /** Transient line ("Rate limited — retrying…"); cleared by the next event. */
  status?: string;
};

/** Map the run's progress onto the AI SDK tool-part lifecycle. */
function toolState(run: Run): ToolUIPart["state"] {
  if (run.result) return run.result.success ? "output-available" : "output-error";
  // A run error while the gate is open also closes it — nothing can fire now.
  if (run.error) return "output-error";
  if (run.confirmed === false) return "output-denied";
  if (run.call?.requiresConfirmation && run.confirmed === undefined)
    return "approval-requested";
  return "input-available";
}

/** The tool call's "field: value" lines as an object, for ToolInput's JSON view. */
function argsObject(args: string[]): Record<string, string> {
  return Object.fromEntries(
    args.map((line) => {
      const i = line.indexOf(": ");
      return i === -1 ? [line, ""] : [line.slice(0, i), line.slice(i + 2)];
    }),
  );
}

/**
 * The agent panel, in the same card chrome as the ask panel. Shows what was
 * heard, surfaces the tool call it resolved to, asks to confirm a write
 * before it fires, then reports the result.
 */
function Agent() {
  const [run, setRun] = useState<Run>({ heard: "" });
  const [copied, setCopied] = useState(false);
  const cardRef = useRef<HTMLDivElement>(null);

  usePanelAutosize(cardRef);

  useEffect(() => {
    const unlisten = events.agent.listen(({ payload }) => {
      switch (payload.kind) {
        case "runStarted":
          // A new run resets the panel; it is reused, not recreated.
          setRun({ heard: payload.value });
          setCopied(false);
          break;
        case "toolCall":
          setRun((r) => ({ ...r, call: payload.value, status: undefined }));
          break;
        case "toolResult":
          setRun((r) => ({ ...r, result: payload.value, status: undefined }));
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
    setRun((r) => ({ ...r, confirmed: approved }));
    commands.confirmAgent(approved);
  };

  const state = toolState(run);
  const copyText = run.finished || run.result?.message || "";

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
        {/* Before a tool call resolves, we're still parsing the transcript. */}
        {!run.call && !run.error && !run.status && (
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader />
            Understanding…
          </div>
        )}

        {run.call && (
          <Tool defaultOpen className="mb-0">
            <ToolHeader
              type={`tool-${run.call.name}`}
              title={run.call.name}
              state={state}
            />
            <ToolContent>
              <ToolInput input={argsObject(run.call.args)} />
              <ToolOutput
                output={
                  run.result?.success ? (
                    <MessageResponse className="p-3">
                      {run.result.message}
                    </MessageResponse>
                  ) : undefined
                }
                errorText={
                  run.result && !run.result.success ? run.result.message : undefined
                }
              />
            </ToolContent>
          </Tool>
        )}

        {run.call && state === "approval-requested" && (
          <Confirmation approval={{ id: run.call.name }} state={state}>
            <ConfirmationTitle>
              Run <span className="font-mono text-[13px]">{run.call.name}</span>?
              This will write to {run.call.app}.
            </ConfirmationTitle>
            <ConfirmationRequest>
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
          {run.call ? `· ${run.call.name}` : ""}
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
