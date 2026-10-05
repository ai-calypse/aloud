import { useState, type RefObject } from "react";
import { CheckIcon, CopyIcon, PlusIcon, Trash2Icon } from "lucide-react";
import type { Entry } from "@/lib/bindings";
import { withDecision, type Run } from "@/lib/agent-steps";
import { Button } from "@/components/ui/button";
import { AppIcon } from "@/components/notch/app-icon";
import { Empty, Label, List, Row, Value } from "@/components/notch/list";
import { AgentDetailBody, AgentList } from "@/features/overlay/AgentList";
import { AgentRun } from "@/features/overlay/AgentRun";
import { CodingAgentComposer } from "@/features/overlay/CodingAgentComposer";
import { QuotaDashboard } from "@/features/overlay/QuotaDashboard";
import { cn } from "@/lib/utils";
import { useNotch } from "../context";
import { justFinished, liveRun, voiceRow, type VoiceRow } from "../voice-runs";

/**
 * Agents › Voice: what you have said to Aloud, newest first — dictations and
 * commands alike — with the command running now on top of them, steps and
 * approval gate included.
 */
export function AgentsVoice() {
  const { agent, setAgent, runs, removeRun, bodyRef } = useNotch();
  return (
    <VoiceRuns
      run={agent}
      runs={runs}
      bodyRef={bodyRef}
      onDecide={(confirmed) =>
        setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) })
      }
      onDelete={removeRun}
    />
  );
}

export function VoiceRuns({
  run,
  runs,
  bodyRef,
  onDecide,
  onDelete,
  now,
}: {
  run: Run | null;
  runs: Entry[];
  bodyRef: RefObject<HTMLDivElement | null>;
  onDecide: (confirmed: boolean) => void;
  onDelete: (id: number) => void;
  now?: number;
}) {
  const live = liveRun(run, runs);
  // undefined until the user picks: the run that just ended starts open.
  const [picked, setPicked] = useState<number | null | undefined>(undefined);
  const open = picked === undefined ? justFinished(run, runs) : picked;
  return (
    <>
      {live && (
        <AgentRun key={live.heard} run={live} bodyRef={bodyRef} onDecide={onDecide} />
      )}
      {runs.length === 0 ? (
        !live && (
          <Empty title="Nothing said yet">
            What you say to Aloud, by dictation or as a command, is listed here, newest first.
            Hold the Command key and say what to do.
          </Empty>
        )
      ) : (
        <>
          <Label className={live ? "mt-3" : undefined}>{live ? "Before this" : "Recent"}</Label>
          <List label="Past voice runs">
            {runs.map((entry) => (
              <VoiceRunRow
                key={entry.id}
                row={voiceRow(entry, now)}
                open={open === entry.id}
                onToggle={() => setPicked(open === entry.id ? null : entry.id)}
                onDelete={() => onDelete(entry.id)}
              />
            ))}
          </List>
        </>
      )}
    </>
  );
}

const KIND_LABEL: Record<Entry["kind"], string> = { dictation: "Dictation", command: "Command" };

function VoiceRunRow({
  row,
  open,
  onToggle,
  onDelete,
}: {
  row: VoiceRow;
  open: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <Row
      lead={<AppIcon icon={row.kind === "dictation" ? "mic" : "agents"} label={KIND_LABEL[row.kind]} />}
      title={row.said || "(nothing heard)"}
      subtitle={<span className={cn(row.failed && "text-notch-danger")}>{row.result}</span>}
      trailing={<Value>{[row.app, row.when].filter(Boolean).join(" · ")}</Value>}
      expanded={open}
      onClick={onToggle}
      detail={
        open && (
          <div className="pr-2 pb-2.5 pl-[44px]">
            {/* ponytail: plain text, so an answer's markdown shows as typed;
                the finished run's own view (AgentRun) renders it while it is live. */}
            <p className="m-0 text-notch-body whitespace-pre-wrap text-notch-ink-2 select-text">
              {row.full || row.result}
            </p>
            <div className="mt-2 flex gap-1">
              <Button
                variant="notch-quiet"
                size="notch"
                className="-ml-2.5"
                onClick={() =>
                  void navigator.clipboard.writeText(row.full || row.said).then(() => setCopied(true))
                }
              >
                {copied ? <CheckIcon /> : <CopyIcon />}
                {copied ? "Copied" : "Copy"}
              </Button>
              <Button variant="notch-quiet" size="notch" onClick={onDelete}>
                <Trash2Icon />
                Delete
              </Button>
            </div>
          </div>
        )
      }
    />
  );
}

/**
 * Agents › Coding: Claude Code and Codex sessions, one drilled into, and the
 * approval cards they raise — under the row of the agent that is asking.
 */
export function AgentsCoding() {
  const { sessions, permissions, selected, select, resolvePermission, showPeek, bodyRef } =
    useNotch();
  // Local: leaving the subsection drops a half-filled form, which is what
  // closing the old tab did too.
  const [composing, setComposing] = useState(false);
  const session = sessions.find((s) => s.id === selected);
  if (composing) {
    return <CodingAgentComposer bodyRef={bodyRef} onClose={() => setComposing(false)} />;
  }
  if (session) {
    return (
      <AgentDetailBody
        session={session}
        permission={permissions.find((p) => p.sessionId === session.id) ?? null}
        onResolve={resolvePermission}
        onBack={() => select(null)}
        onPeek={showPeek}
      />
    );
  }
  return (
    <>
      <div className="mb-1 flex justify-end">
        <Button variant="notch-quiet" size="notch" onClick={() => setComposing(true)}>
          <PlusIcon />
          New coding agent
        </Button>
      </div>
      <AgentList
        sessions={sessions}
        permissions={permissions}
        onSelect={select}
        onResolve={resolvePermission}
        onPeek={showPeek}
      />
    </>
  );
}

/** Agents › Usage: how much of the plan the coding agents have used. */
export function AgentsUsage() {
  return <QuotaDashboard />;
}
