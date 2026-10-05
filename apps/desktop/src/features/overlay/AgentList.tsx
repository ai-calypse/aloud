import { useEffect, useState } from "react";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type AgentTranscript,
  type AgentTranscriptEntry,
  type ApprovalReply,
} from "@/lib/bindings";
import { Empty } from "@/components/notch/list";
import {
  agentStage,
  agentHue,
  stageSeed,
  type AgentStage,
} from "@/lib/agent-stage";
import { PermissionCard } from "./PermissionCards";
import {
  AgentAvatar,
  rowState,
  rowName,
  rowTag,
  type PeekFn,
} from "./agentIdentity";

/**
 * The stage cat as a round avatar with a corner status badge, and the hover that
 * pops the meme up large.
 */
function StageAvatar({
  session,
  stage,
  state,
  onPeek,
}: {
  session: AgentSession;
  stage: AgentStage;
  state: "busy" | "live" | "done";
  onPeek: PeekFn;
}) {
  // One seed for both the avatar and its peek, so they never disagree — it is
  // fixed while the agent stays in this stage and changes on re-entry.
  const seed = stageSeed(session.id, stage);
  return (
    <span
      className="agent-row__avatar"
      onMouseEnter={(e) => onPeek(stage, seed, e.currentTarget)}
      onMouseLeave={() => onPeek(null, "", null)}
    >
      <AgentAvatar
        className="agent-row__cat"
        stage={stage}
        seed={seed}
        hue={agentHue(session.id)}
      />
      <span className="agent-row__badge" data-state={state} aria-hidden />
    </span>
  );
}

/**
 * One session in the list: the agent in bold, a `model · time` tag coloured by
 * state, the prompt or running tool, and a subagent count. The whole row is a
 * button into the detail view, and it lights up when that agent is asking.
 *
 * Ported from `agent-notch`'s `row(for:)`, made clickable.
 */
function AgentRow({
  session,
  asking,
  onSelect,
  onPeek,
}: {
  session: AgentSession;
  asking: boolean;
  onSelect: (id: string) => void;
  onPeek: PeekFn;
}) {
  const state = rowState(session);
  const stage = agentStage(session, asking);
  return (
    <li>
      <button
        className="agent-row agent-row--button"
        data-asking={asking || undefined}
        onClick={() => onSelect(session.id)}
      >
        <div className="agent-row__lead">
          <StageAvatar session={session} stage={stage} state={state} onPeek={onPeek} />
          <div className="agent-row__body">
            <div className="agent-row__head">
              <span className="agent-row__name">{rowName(session)}</span>
              <span className="agent-row__tag" data-state={state}>
                {rowTag(session)}
              </span>
            </div>
            <p className="agent-row__prompt">
          {/* When asking, that is the most important thing about the row.
              Otherwise the running tool, else the user's own latest words —
              never the ai-title under a "You:" label; the title is the
              model's and is written once at session start. */}
              {asking
                ? "⚠ wants permission"
                : (session.tool ??
                  (session.prompt ? `You: ${session.prompt}` : (session.title ?? "")))}
            </p>
            {session.subagents.length > 0 && (
              <p className="agent-row__subcount">
                ▸ {session.subagents.length} subagent{session.subagents.length === 1 ? "" : "s"}
              </p>
            )}
          </div>
        </div>
      </button>
    </li>
  );
}

/**
 * The list of running agents. Each row summarises one agent; clicking it drills
 * into a live detail view; a row whose agent is asking permission is flagged.
 * Chrome-less: it is the notch's Agents › Coding view, under the shell's own
 * tabs, so a title and a close of its own would only repeat them.
 */
export function AgentList({
  sessions,
  permissions,
  onSelect,
  onResolve,
  onPeek,
}: {
  sessions: AgentSession[];
  permissions: AgentPermission[];
  onSelect: (id: string) => void;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onPeek: PeekFn;
}) {
  const askingIds = new Set(permissions.map((p) => p.sessionId).filter(Boolean));
  return (
    <>
      {sessions.length === 0 ? (
        <Empty title="No coding agents running">
          Claude Code and Codex sessions show up here while they work, with any
          approval they are waiting on.
        </Empty>
      ) : (
        <ul className="agents">
          {sessions.map((session) => (
            <AgentRow
              session={session}
              asking={askingIds.has(session.id)}
              onSelect={onSelect}
              onPeek={onPeek}
              key={session.id}
            />
          ))}
        </ul>
      )}
      {/* A permission for an agent not in the list (rare — a hooked session the
          poll has not folded yet) still needs answering, so it falls through
          here rather than being lost. */}
      {permissions
        .filter((p) => !p.sessionId || !sessions.some((s) => s.id === p.sessionId))
        .map((p) => (
          <PermissionCard key={p.id} request={p} queued={0} onResolve={onResolve} />
        ))}
    </>
  );
}

/** A session's transcript, read once per session when `enabled`; shared with the coding detail board. */
export function useTranscript(sessionId: string, enabled: boolean) {
  const [transcript, setTranscript] = useState<AgentTranscript | null>(null);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
  useEffect(() => {
    if (!enabled) return;
    setTranscript(null);
    setTranscriptError(null);
    let live = true;
    void commands.readAgentTranscript(sessionId).then((res) => {
      if (!live) return;
      if (res.status === "ok") setTranscript(res.data);
      else setTranscriptError(res.error);
    });
    return () => {
      live = false;
    };
  }, [enabled, sessionId]);
  return { transcript, transcriptError };
}

/** One collapsed line of tool activity. */
export type Tick = { text: string; tool: string | null; count: number; isError: boolean; title: string };

/** The recent tool calls, most-recent last, collapsed hierarchically: a run of
 *  the same tool folds into one tick with a count, so a burst of reads reads as
 *  "Read ×5", not five lines. Grouped on the structured `tool` field, not by
 *  re-parsing the rendered prose. A single call keeps its full prose ("Reading
 *  x.rs"); a folded run shows the tool name. Capped to the last few. */
export function toolTicks(entries: AgentTranscriptEntry[], max = 6): Tick[] {
  const tools = entries.filter((e) => e.kind === "toolUse");
  const ticks: Tick[] = [];
  for (const t of tools) {
    const last = ticks[ticks.length - 1];
    if (last && !t.isError && !last.isError && t.tool != null && last.tool === t.tool) {
      // Fold into the run: the tool name stands in for the individual targets,
      // and the count (rendered separately) says how many.
      last.count += 1;
      last.text = t.tool;
      last.title = `${last.title}\n${t.result ?? t.text}`;
    } else {
      ticks.push({
        text: t.text,
        tool: t.tool,
        count: 1,
        isError: t.isError,
        title: t.result ?? t.text,
      });
    }
  }
  return ticks.slice(-max);
}
