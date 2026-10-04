import { withDecision } from "@/lib/agent-steps";
import { Button } from "@/components/ui/button";
import { Empty } from "@/components/notch/list";
import { AgentDetailBody, AgentList } from "@/features/overlay/AgentList";
import { AgentRun } from "@/features/overlay/AgentRun";
import { CodingAgentComposer } from "@/features/overlay/CodingAgentComposer";
import { QuotaDashboard } from "@/features/overlay/QuotaDashboard";
import { MAX_TABS, TabStrip } from "@/features/overlay/TabStrip";
import { useNotch } from "../context";

/**
 * Agents › Voice: the command you just spoke, its steps and its approval
 * gate, with the run's tab strip when coding-agent composers are open beside
 * it. The strip stays away until there is a second tab — a lone run reads
 * better without a tab bar naming it a third time (section, subsection, tab).
 */
export function AgentsVoice() {
  const { agent, setAgent, voice, bodyRef } = useNotch();
  const composing = voice.tabs.find((tab) => tab.id === voice.active)?.blank;
  const addTab = voice.tabs.length < MAX_TABS ? voice.add : undefined;

  return (
    <>
      {voice.tabs.some((tab) => tab.blank) && (
        <TabStrip
          tabs={voice.tabs}
          active={voice.active}
          runOpen={agent !== null}
          title={agent?.heard ?? "Aloud"}
          onSelect={voice.select}
          onClose={voice.close}
          onClosed={voice.drop}
          onAdd={voice.add}
        />
      )}
      {composing ? (
        // Keyed on the tab so switching back replays its entrance.
        <CodingAgentComposer
          key={voice.active}
          bodyRef={bodyRef}
          onClose={() => voice.close(voice.active)}
          onAddTab={addTab}
        />
      ) : agent ? (
        <AgentRun
          key={agent.heard}
          run={agent}
          bodyRef={bodyRef}
          onAddTab={addTab}
          onDecide={(confirmed) =>
            setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) })
          }
        />
      ) : (
        <Empty
          title="No command running"
          action={
            addTab && (
              <Button variant="notch-quiet" size="notch" className="-ml-2.5" onClick={addTab}>
                Start a coding agent
              </Button>
            )
          }
        >
          Hold the Command key and say what to do. Its steps, and anything that
          needs your yes, show up here.
        </Empty>
      )}
    </>
  );
}

/**
 * Agents › Coding: Claude Code and Codex sessions, one drilled into, and the
 * approval cards they raise — under the row of the agent that is asking.
 */
export function AgentsCoding() {
  const { sessions, permissions, selected, select, resolvePermission, showPeek } = useNotch();
  const session = sessions.find((s) => s.id === selected);
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
    <AgentList
      sessions={sessions}
      permissions={permissions}
      onSelect={select}
      onResolve={resolvePermission}
      onPeek={showPeek}
    />
  );
}

/** Agents › Usage: how much of the plan the coding agents have used. */
export function AgentsUsage() {
  return <QuotaDashboard />;
}
