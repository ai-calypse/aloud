import { useState } from "react";
import { PlusIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { AgentList } from "@/features/overlay/AgentList";
import { CodingAgentComposer } from "@/features/overlay/CodingAgentComposer";
import { QuotaDashboard } from "@/features/overlay/QuotaDashboard";
import { useNotch } from "../context";
import { CodingDetail } from "./widgets/Agents";

/**
 * Agents › Coding: Claude Code and Codex sessions as a list, the approval
 * cards they raise under the row of the agent that is asking. A session opens
 * to its widget board (`CodingDetail`): the activity, the session and its
 * subagents as tiles.
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
      <CodingDetail
        session={session}
        permission={permissions.find((p) => p.sessionId === session.id) ?? null}
        queued={permissions.filter((p) => p.sessionId !== session.id).length}
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
