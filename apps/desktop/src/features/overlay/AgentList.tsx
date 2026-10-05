import { useEffect, useState } from "react";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type AgentTranscript,
  type AgentTranscriptEntry,
  type ApprovalReply,
} from "@/lib/bindings";
import { MessageResponse } from "@/components/ai-elements/message";
import { Empty } from "@/components/notch/list";
import {
  agentStage,
  agentHue,
  stageSeed,
  type AgentStage,
} from "@/lib/agent-stage";
import { formatCost, tokenSummary, truncate } from "@/lib/format";
import { baseName, PermissionCard } from "./PermissionCards";
import {
  AgentAvatar,
  rowState,
  rowName,
  rowTag,
  type PeekFn,
} from "./agentIdentity";

/**
 * The user's prompt can be a whole paragraph; capped so it never fills the
 * notch. The full text is on the `<p>`'s title, and the detail view a click
 * opens is where the rest lives.
 */
const PROMPT_CHARS = 140;

/**
 * The stage cat as a round avatar with a corner status badge, and the hover that
 * pops the meme up large. One component so the list row and the drilled-in
 * detail show the exact same thing — they used to carry byte-identical copies.
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

/** A session's transcript, read once per session when `enabled`; shared with the widgets look. */
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

/**
 * One agent in full: avatar + tag, cwd, prompt, the approve/deny card if it is
 * asking (so the decision sits with the terminal it belongs to), the live
 * "Now" tool, and every subagent: Agents › Coding, drilled into.
 *
 * Reads live: it is handed the session straight from the `agentSessions`
 * state, which the event listener replaces on every poll.
 *
 * `onBack` renders the "‹ All agents" link only when given.
 */
export function AgentDetailBody({
  session,
  permission,
  onResolve,
  onBack,
  onPeek,
}: {
  session: AgentSession;
  permission: AgentPermission | null;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onBack?: () => void;
  onPeek: PeekFn;
}) {
  const state = rowState(session);
  const stage = agentStage(session, permission !== null);

  // The transcript backs both the usage line (in the meta) and the thread list.
  // Only the clicked-in detail (onBack) reads it; the hover peek stays compact.
  const { transcript, transcriptError } = useTranscript(session.id, Boolean(onBack));

  return (
    <div className="agent-detail">
      {onBack && (
        <button className="agent-detail__back" onClick={onBack}>
          ‹ All agents
        </button>
      )}
      {/* The same stage avatar as the list, so the detail view matches. The
          live state sits right here beside it — a small dot + word — instead of
          taking its own row below. */}
      <div className="agent-detail__meta">
        <StageAvatar session={session} stage={stage} state={state} onPeek={onPeek} />
        <div className="agent-detail__id">
          <span className="agent-detail__tag" data-state={state}>
            {rowTag(session)}
          </span>
          <span className="agent-detail__state" data-state={state}>
            <span className="agent-detail__state-dot" aria-hidden />
            {session.tool ?? (session.busy ? "thinking…" : "idle")}
          </span>
        </div>
      </div>
      {/* Folder + usage share a line below the header, so big token counts
          don't crowd the model tag. Just the folder name, not the full path —
          the leaf is what identifies the session. */}
      {(session.cwd || (transcript && transcript.entries.length > 0)) && (
        <div className="agent-detail__subline">
          {session.cwd && (
            <span className="agent-detail__cwd" title={session.cwd}>
              {baseName(session.cwd)}
            </span>
          )}
          {transcript && transcript.entries.length > 0 && (
            <span className="agent-detail__usage">
              {transcript.costUsd != null && transcript.costUsd > 0 && (
                <span className="agent-detail__cost">{formatCost(transcript.costUsd)}</span>
              )}
              {tokenSummary(
                transcript.inputTokens,
                transcript.cacheReadTokens,
                transcript.outputTokens,
              )}
            </span>
          )}
        </div>
      )}
      {session.prompt ? (
        <p className="agent-detail__prompt" title={session.prompt}>
          You: {truncate(session.prompt, PROMPT_CHARS)}
        </p>
      ) : (
        session.title && (
          <p className="agent-detail__prompt" title={session.title}>
            {truncate(session.title, PROMPT_CHARS)}
          </p>
        )
      )}

      {/* The card for this exact agent — the whole point of the detail view. */}
      {permission && (
        <PermissionCard request={permission} queued={0} onResolve={onResolve} />
      )}

      {/* The full thread — only in the clicked-in detail. Usage now lives up in
          the meta line; this is just the collapsible conversation. */}
      {onBack && <AgentTranscriptView data={transcript} error={transcriptError} />}

      {session.subagents.length > 0 && (
        <div className="agent-detail__subs">
          <span className="agent-detail__label">
            Subagents ({session.subagents.length})
          </span>
          <ul className="subagents">
            {session.subagents.map((sub, i) => (
              <li className="subagent-row" key={i}>
                <span
                  className="agent-row__state"
                  data-state={sub.busy ? "busy" : "done"}
                  aria-hidden
                />
                <span className="subagent-row__label">{sub.label}</span>
                {sub.tool && <span className="subagent-row__tool">{sub.tool}</span>}
                {sub.model && <span className="subagent-row__model">{sub.model}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/**
 * The session's thread as one interleaved stream: your prompts, Claude's
 * messages (markdown), and tool calls in the order they happened, newest at the
 * bottom. Always shown — the `.jsonl` read is already lazy (on detail open), and
 * the parser caps at the last 20 entries so this stays a recent-activity window.
 */
/**
 * The activity view — deliberately spare. Not the whole thread: Claude's
 * *latest* message, and a short list of the *recent* tool calls as ticks. The
 * full transcript is one glance's worth of "what's it saying + what did it just
 * do", not an archive to scroll.
 */
function AgentTranscriptView({
  data,
  error,
}: {
  data: AgentTranscript | null;
  error: string | null;
}) {
  if (error) {
    return <p className="agent-detail__transcript-empty">{error}</p>;
  }
  if (!data) {
    return <p className="agent-detail__transcript-empty">Reading…</p>;
  }
  if (data.entries.length === 0) {
    return <p className="agent-detail__transcript-empty">No activity yet.</p>;
  }

  // The last assistant message, without copying the array (findLast needs a
  // newer TS lib target than the project uses).
  let latest: AgentTranscriptEntry | undefined;
  for (let i = data.entries.length - 1; i >= 0; i--) {
    if (data.entries[i].kind === "assistant") {
      latest = data.entries[i];
      break;
    }
  }
  const ticks = toolTicks(data.entries);

  return (
    <div
      className="activity"
      // Keep the newest content in view as it updates — the box scrolls
      // internally, so the panel (and the notch) never resizes.
      ref={(el) => {
        if (!el) return;
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            el.scrollTop = el.scrollHeight;
          }),
        );
      }}
    >
      {latest && <AssistantRow text={latest.text} />}
      {ticks.length > 0 && (
        <ul className="ticks">
          {ticks.map((tick, i) => (
            <li className="tick" data-error={tick.isError || undefined} key={i}>
              <span className="tick__mark" aria-hidden>
                {tick.isError ? "✗" : "✓"}
              </span>
              <span className="tick__text" title={tick.title}>
                {tick.text}
              </span>
              {tick.count > 1 && <span className="tick__count">×{tick.count}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
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

/**
 * Claude's latest message, as markdown. No clamp or "Show more" — the activity
 * box is a fixed-height scroll area, so a long message just scrolls in place
 * rather than growing the panel (which would re-animate the notch). Plain: no
 * per-message measuring, which was itself a source of height jitter.
 */
function AssistantRow({ text }: { text: string }) {
  return (
    <div className="msg">
      <MessageResponse>{text}</MessageResponse>
    </div>
  );
}
