import { useState, type RefObject } from "react";
import {
  BotIcon,
  CheckIcon,
  CopyIcon,
  LoaderCircleIcon,
  MicIcon,
  PlusIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type AgentTranscript,
  type ApprovalReply,
} from "@/lib/bindings";
import { groupSteps, pendingStep, stepStatus, withDecision, type Run, type Step } from "@/lib/agent-steps";
import { agentHue, agentStage, stageSeed, type AgentStage } from "@/lib/agent-stage";
import { formatCost, tokenSummary } from "@/lib/format";
import { labelFor, rowLabel, text } from "@/lib/tool-views";
import { cn } from "@/lib/utils";
import { MessageResponse } from "@/components/ai-elements/message";
import { Shimmer } from "@/components/ai-elements/shimmer";
import { PreviewCard } from "@/components/preview/cards";
import {
  Numeral,
  Ring,
  WidgetBoard,
  WidgetFrame,
  type WidgetRole,
} from "@/components/notch/widgets";
import { toolTicks, useTranscript } from "@/features/overlay/AgentList";
import { AskGate, ConfirmGate, FinderGrid, ShowButton, UndoButton } from "@/features/overlay/AgentRun";
import { AGENT_NAME, AgentAvatar, rowName, rowTag, type PeekFn } from "@/features/overlay/agentIdentity";
import { CodingAgentComposer } from "@/features/overlay/CodingAgentComposer";
import { PermissionCard, baseName } from "@/features/overlay/PermissionCards";
import { useQuota } from "@/features/overlay/QuotaDashboard";
import { MAX_TABS, TabStrip } from "@/features/overlay/TabStrip";
import { useNotch } from "../../context";
import {
  extraUsage,
  runSummary,
  sessionLabel,
  sessionState,
  stepRole,
  usageBars,
  type UsageBar,
} from "./agents-logic";
import { Idle, Pill } from "./agents-parts";

/** The word on a row or a tile: in its accent's ink when it has one, quiet when it does not. */
const wordClass = (role: WidgetRole | undefined) => (role ? "text-widget-acc-ink" : "text-widget-ink-2");

/* ---------------------------------------------------------------------------
 * Agents › Voice
 * ------------------------------------------------------------------------- */

/**
 * Agents › Voice in the widgets look. The tab strip and the composer for a new
 * coding agent are the calm ones: they are a tab bar and a text form, not a
 * view of data, and a second copy would drift. The run itself is a board: the
 * command and its steps in one Large tile, then either the approval it is
 * waiting on or its answer, how far it got and where it stands.
 */
export function WidgetVoice() {
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
        <CodingAgentComposer
          key={voice.active}
          bodyRef={bodyRef}
          onClose={() => voice.close(voice.active)}
          onAddTab={addTab}
        />
      ) : agent ? (
        <VoiceRun
          key={agent.heard}
          run={agent}
          bodyRef={bodyRef}
          onAddTab={addTab}
          onDecide={(confirmed) =>
            setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) })
          }
        />
      ) : (
        <WidgetBoard label="Voice command">
          <WidgetFrame size="medium" label="Voice command">
            <Idle icon={<MicIcon strokeWidth={1.6} />} title="No command running">
              Hold the Command key and say what to do. Its steps, and anything that needs your yes, show up here.
              {addTab && (
                <span className="mt-2 flex">
                  <Pill quiet onClick={addTab}>
                    Start a coding agent
                  </Pill>
                </span>
              )}
            </Idle>
          </WidgetFrame>
        </WidgetBoard>
      )}
    </>
  );
}

export function VoiceRun({
  run,
  bodyRef,
  onAddTab,
  onDecide,
}: {
  run: Run;
  bodyRef?: RefObject<HTMLDivElement | null>;
  onAddTab?: () => void;
  onDecide?: (confirmed: boolean) => void;
}) {
  const pending = pendingStep(run);
  const summary = runSummary(run);
  const asking = run.question && !pending ? run.question : null;
  const last = run.steps[run.steps.length - 1];

  return (
    <WidgetBoard label="Voice command">
      <WidgetFrame size="large" label="Voice command">
        <div className="flex h-full flex-col p-3.5">
          <span className="text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
            You said
          </span>
          <p className="m-0 line-clamp-2 text-notch-w-headline leading-tight font-semibold" title={run.heard}>
            {run.heard}
          </p>
          {/* Before any tool call resolves we are still parsing the transcript; a plain
              answer finishes with no steps at all, so `finished` ends this too. */}
          <div ref={bodyRef} className="mt-2 min-h-0 flex-1 overflow-y-auto overscroll-contain">
            {run.steps.length === 0 && !run.error && !run.status && !run.finished && !run.question && (
              <p className="m-0 flex items-center gap-2 text-notch-w-body text-widget-ink-2">
                <LoaderCircleIcon className="size-4 animate-spin" aria-hidden />
                {/* `as="span"`: Shimmer renders a <p> by default, and a <p> in a <p> is invalid. */}
                <Shimmer as="span">Understanding…</Shimmer>
              </p>
            )}
            {(run.steps.length > 0 || (run.planned?.length ?? 0) > 0) && (
              <ul className="m-0 list-none p-0" aria-label="Steps">
                {groupSteps(run.steps, (s) => rowKey(s, s === pending)).map(({ step, count }, i) => (
                  <StepRow key={`${step.name}-${i}`} step={step} count={count} pending={step === pending} />
                ))}
                {run.planned?.map((next, i) => (
                  <li key={`planned-${next.name}-${i}`} className="border-b border-widget-sep py-1.5 last:border-b-0">
                    <div className="flex items-center gap-2 text-widget-ink-2">
                      <span aria-hidden className="size-4 shrink-0 rounded-full border border-widget-radio" />
                      <span className="min-w-0 flex-1 truncate text-notch-w-title font-medium">
                        {labelFor(next.name, {})}
                      </span>
                      <span className="shrink-0 text-notch-w-meta font-medium">queued</span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div className="mt-2 flex items-center gap-1.5">
            {onAddTab && (
              <Pill quiet onClick={onAddTab}>
                <PlusIcon className="size-3.5" strokeWidth={2.5} aria-hidden />
                New tab
              </Pill>
            )}
            {run.finished && <CopyPill text={run.finished} />}
            <Pill quiet className="ml-auto" onClick={() => void commands.closeAgent()}>
              <XIcon className="size-3.5" strokeWidth={2.5} aria-hidden />
              Close
            </Pill>
          </div>
        </div>
      </WidgetFrame>

      {pending ? (
        // The approval card is the app's own and keeps its keys, its countdown and its
        // edits; it is not restyled, only given a tile to sit in.
        <WidgetFrame size="large" label="Needs your yes" accent="orange">
          <div className="h-full overflow-y-auto overscroll-contain p-3">
            <ConfirmGate
              key={`${pending.name}-${pending.preview?.revision ?? run.steps.length}`}
              step={pending}
              onDecide={onDecide}
            />
          </div>
        </WidgetFrame>
      ) : asking ? (
        <WidgetFrame size="large" label="Asking you" accent="orange">
          <div className="h-full overflow-y-auto overscroll-contain p-3">
            <AskGate key={asking.question} asked={asking} />
          </div>
        </WidgetFrame>
      ) : (
        <>
          <WidgetFrame size="medium" label={run.finished || run.error ? "Answer" : "Working on it"}>
            <div className="h-full overflow-y-auto overscroll-contain p-3.5 text-notch-w-body leading-snug">
              {run.status && (
                <p className="m-0 mb-2 flex items-center gap-2 text-widget-ink-2">
                  <LoaderCircleIcon className="size-4 animate-spin" aria-hidden />
                  <Shimmer as="span">{run.status}</Shimmer>
                </p>
              )}
              {run.finished && <MessageResponse>{run.finished}</MessageResponse>}
              {run.error && (
                <p data-w-role="move" className="m-0 font-medium text-widget-acc-ink">
                  {run.error}
                </p>
              )}
              {!run.finished && !run.error && !run.status && (
                <p className="m-0 text-widget-ink-2">The answer shows here when the steps are done.</p>
              )}
            </div>
          </WidgetFrame>
          <WidgetFrame size="small" label="Steps" ground="deep" accent={summary.role}>
            {summary.total === 0 ? (
              <Idle icon={<BotIcon strokeWidth={1.6} />} title="No steps yet" />
            ) : (
              <div className="flex h-full flex-col justify-between p-3.5">
                <Ring
                  value={summary.done / summary.total}
                  label="Steps finished"
                  valueText={`${summary.done} of ${summary.total} steps`}
                  size={56}
                  stroke={9}
                />
                <div>
                  <Numeral text={String(summary.done)} size="s" className="text-widget-acc-ink" />
                  <div className="text-notch-w-title font-medium text-widget-ink-2">
                    of {summary.total} {summary.total === 1 ? "step" : "steps"}
                  </div>
                </div>
              </div>
            )}
          </WidgetFrame>
          <WidgetFrame size="small" label="Status" accent={summary.role} live={summary.word === "Working"}>
            <div className="flex h-full flex-col justify-between p-3.5 text-widget-acc-ink">
              <BotIcon aria-hidden className="size-6" strokeWidth={1.6} />
              <div>
                <div className="text-notch-w-headline leading-tight font-semibold">{summary.word}</div>
                {last && (
                  <div className="mt-0.5 line-clamp-2 text-notch-w-meta leading-snug font-medium text-widget-ink-2">
                    {rowLabel(last.name, last.args, last.preview?.title)}
                  </div>
                )}
              </div>
            </div>
          </WidgetFrame>
        </>
      )}
    </WidgetBoard>
  );
}

function CopyPill({ text: value }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Pill quiet onClick={() => void navigator.clipboard.writeText(value).then(() => setCopied(true))}>
      {copied ? <CheckIcon className="size-3.5" aria-hidden /> : <CopyIcon className="size-3.5" aria-hidden />}
      {copied ? "Copied" : "Copy"}
    </Pill>
  );
}

/**
 * What a step's row says, as one string. Two adjacent steps sharing it render
 * identically, which is exactly when collapsing them hides nothing.
 */
function rowKey(step: Step, pending: boolean): string {
  return `${rowLabel(step.name, step.args, step.preview?.title)}|${stepStatus(step, pending)}`;
}

function StepRow({ step, count, pending }: { step: Step; count: number; pending: boolean }) {
  const role = stepRole(step, pending);
  return (
    <li className="border-b border-widget-sep py-1.5 last:border-b-0">
      <div className="flex items-center gap-2">
        <StepGlyph step={step} pending={pending} />
        <span className="min-w-0 flex-1 truncate text-notch-w-title font-medium">
          {rowLabel(step.name, step.args, step.preview?.title)}
        </span>
        {count > 1 && <span className="shrink-0 text-notch-w-meta text-widget-ink-2">×{count}</span>}
        <span data-w-role={role} className={cn("shrink-0 text-notch-w-meta font-medium", wordClass(role))}>
          {stepStatus(step, pending)}
        </span>
      </div>
      {(step.result?.show || step.result?.tidied) && (
        <div className="mt-1 flex gap-1 pl-6">
          {step.result.show && <ShowButton show={step.result.show} />}
          {step.result.tidied && <UndoButton folder={step.result.tidied} />}
        </div>
      )}
      {/* Why a saved grant refused it, kept on the row it belongs to. */}
      {step.result?.note && (
        <p data-w-role="orange" className="m-0 mt-0.5 pl-6 text-notch-w-meta text-widget-acc-ink">
          {step.result.note}
        </p>
      )}
      {/* What a write did, as a compact echo of the card it was approved from; never for
          the step awaiting a decision, whose gate is on screen beside it. */}
      {!pending &&
        (step.name === "ALOUD_FINDER_SEARCH" ? (
          <FinderGrid query={text(step.args, "query")} kind={text(step.args, "kind")} found={step.found} />
        ) : (
          step.preview && (
            <div className="mt-1">
              <PreviewCard preview={step.preview} app={step.app} icon={step.icon} compact />
            </div>
          )
        ))}
    </li>
  );
}

function StepGlyph({ step, pending }: { step: Step; pending: boolean }) {
  const cls = "size-4 shrink-0";
  // Before the result: a declined write's result is a failure, but the user said no.
  if (step.confirmed === false) return <XIcon className={cn(cls, "text-widget-ink-2")} aria-hidden />;
  if (step.result) {
    return step.result.success ? (
      <CheckIcon className={cn(cls, "text-widget-ink-2")} aria-hidden />
    ) : (
      <XIcon data-w-role="move" className={cn(cls, "text-widget-acc-ink")} aria-hidden />
    );
  }
  if (pending) return <TriangleAlertIcon data-w-role="orange" className={cn(cls, "text-widget-acc-ink")} aria-hidden />;
  return <LoaderCircleIcon className={cn(cls, "animate-spin text-widget-ink-2")} aria-hidden />;
}

/* ---------------------------------------------------------------------------
 * Agents › Coding
 * ------------------------------------------------------------------------- */

/**
 * Agents › Coding in the widgets look: a Medium tile per session (its cat, its
 * state in words, what it is doing) that opens the session, and a Large tile
 * for the approval the first waiting request needs. One card at a time: every
 * permission card answers Enter for itself, so two on screen would approve both.
 */
export function WidgetCoding() {
  const { sessions, permissions, selected, select, resolvePermission, showPeek } = useNotch();
  const session = sessions.find((s) => s.id === selected);
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
    <CodingList
      sessions={sessions}
      permissions={permissions}
      onSelect={select}
      onResolve={resolvePermission}
      onPeek={showPeek}
    />
  );
}

export function CodingList({
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
    <WidgetBoard label="Coding agents">
      {permissions[0] && (
        <ApprovalTile request={permissions[0]} queued={permissions.length - 1} onResolve={onResolve} />
      )}
      {sessions.length === 0 ? (
        <WidgetFrame size="medium" label="Coding agents">
          <Idle icon={<BotIcon strokeWidth={1.6} />} title="No coding agents running">
            Claude Code and Codex sessions show up here while they work, with any approval they are waiting on.
          </Idle>
        </WidgetFrame>
      ) : (
        sessions.map((session) => (
          <SessionTile
            key={session.id}
            session={session}
            asking={askingIds.has(session.id)}
            onOpen={() => onSelect(session.id)}
            onPeek={onPeek}
          />
        ))
      )}
    </WidgetBoard>
  );
}

/** A waiting request as a Large tile; `queued` is how many more are behind it. */
function ApprovalTile({
  request,
  queued,
  onResolve,
}: {
  request: AgentPermission;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
}) {
  return (
    <WidgetFrame
      size="large"
      label={`${AGENT_NAME[request.agent] ?? request.agent} needs your yes`}
      accent="orange"
    >
      <div className="h-full overflow-y-auto overscroll-contain p-3">
        <PermissionCard key={request.id} request={request} queued={queued} onResolve={onResolve} />
      </div>
    </WidgetFrame>
  );
}

function SessionAvatar({
  session,
  stage,
  onPeek,
}: {
  session: AgentSession;
  stage: AgentStage;
  onPeek: PeekFn;
}) {
  // One seed for the avatar and its peek, fixed while the agent stays in this stage.
  const seed = stageSeed(session.id, stage);
  return (
    <span
      className="size-16 shrink-0 overflow-hidden rounded-full"
      onMouseEnter={(e) => onPeek(stage, seed, e.currentTarget)}
      onMouseLeave={() => onPeek(null, "", null)}
    >
      <AgentAvatar className="size-full object-cover" stage={stage} seed={seed} hue={agentHue(session.id)} />
    </span>
  );
}

export function SessionTile({
  session,
  asking,
  onOpen,
  onPeek,
  extra,
}: {
  session: AgentSession;
  asking: boolean;
  onOpen?: () => void;
  onPeek: PeekFn;
  /** More under the state: the detail's folder and usage. */
  extra?: React.ReactNode;
}) {
  const stage = agentStage(session, asking);
  const state = sessionState(session, asking);
  return (
    <WidgetFrame
      size="medium"
      label={sessionLabel(session, AGENT_NAME)}
      accent={state.role}
      live={asking}
      onActivate={onOpen}
    >
      <div className="flex h-full gap-3 p-3.5">
        <SessionAvatar session={session} stage={stage} onPeek={onPeek} />
        <div className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
          <span className="truncate text-notch-w-over font-semibold tracking-wide text-widget-ink-2 uppercase">
            {rowTag(session)}
          </span>
          <span className="truncate text-notch-w-headline leading-tight font-semibold">{rowName(session)}</span>
          <span className={cn("text-notch-w-title font-semibold", wordClass(state.role))}>{state.word}</span>
          <span className="line-clamp-2 text-notch-w-meta leading-snug text-widget-ink-2">{state.detail}</span>
          {session.subagents.length > 0 && (
            <span className="text-notch-w-meta text-widget-ink-2">
              {session.subagents.length} subagent{session.subagents.length === 1 ? "" : "s"}
            </span>
          )}
          {extra}
        </div>
      </div>
    </WidgetFrame>
  );
}

export function CodingDetail({
  session,
  permission,
  queued,
  onResolve,
  onBack,
  onPeek,
  transcript,
  transcriptError,
}: {
  session: AgentSession;
  permission: AgentPermission | null;
  queued: number;
  onResolve: (id: string, reply: ApprovalReply) => void;
  onBack: () => void;
  onPeek: PeekFn;
  /** Given by the fixtures; the live view reads it itself. */
  transcript?: AgentTranscript | null;
  transcriptError?: string | null;
}) {
  const live = useTranscript(session.id, transcript === undefined);
  const data = transcript === undefined ? live.transcript : transcript;
  const error = transcript === undefined ? live.transcriptError : (transcriptError ?? null);
  const usage =
    data && data.entries.length > 0
      ? [
          data.costUsd != null && data.costUsd > 0 ? formatCost(data.costUsd) : null,
          tokenSummary(data.inputTokens, data.cacheReadTokens, data.outputTokens),
        ]
          .filter(Boolean)
          .join(" · ")
      : null;
  const folder = session.cwd ? baseName(session.cwd) : null;

  return (
    <>
      <div className="notch-board-bar mb-3">
        <Pill quiet onClick={onBack}>
          ‹ All agents
        </Pill>
      </div>
      <WidgetBoard label={`${rowName(session)} in detail`}>
        {permission && <ApprovalTile request={permission} queued={queued} onResolve={onResolve} />}
        <ActivityTile data={data} error={error} />
        <SessionTile
          session={session}
          asking={permission !== null}
          onPeek={onPeek}
          extra={
            (folder || usage) && (
              <span className="line-clamp-2 text-notch-w-meta leading-snug text-widget-ink-2" title={session.cwd ?? undefined}>
                {[folder, usage].filter(Boolean).join(" · ")}
              </span>
            )
          }
        />
        {session.subagents.length > 0 && (
          <WidgetFrame size="medium" label={`Subagents (${session.subagents.length})`}>
            <ul className="m-0 h-full list-none overflow-y-auto overscroll-contain p-3.5">
              {session.subagents.map((sub, i) => (
                <li
                  key={i}
                  className="flex items-center gap-2 border-b border-widget-sep py-1.5 text-notch-w-title first:pt-0 last:border-b-0"
                >
                  <span className="min-w-0 flex-1 truncate font-medium">{sub.label}</span>
                  {sub.tool && <span className="truncate text-notch-w-meta text-widget-ink-2">{sub.tool}</span>}
                  <span className="shrink-0 text-notch-w-meta font-medium text-widget-ink-2">
                    {sub.busy ? "Running" : "Done"}
                  </span>
                </li>
              ))}
            </ul>
          </WidgetFrame>
        )}
      </WidgetBoard>
    </>
  );
}

/** Claude's latest message and the last few tool calls: one glance's worth, not an archive. */
function ActivityTile({ data, error }: { data: AgentTranscript | null; error: string | null }) {
  let body: React.ReactNode;
  if (error) body = <p className="m-0 text-widget-ink-2">{error}</p>;
  else if (!data) body = <p className="m-0 text-widget-ink-2">Reading…</p>;
  else if (data.entries.length === 0) body = <p className="m-0 text-widget-ink-2">No activity yet.</p>;
  else {
    let latest: string | null = null;
    for (let i = data.entries.length - 1; i >= 0; i--) {
      if (data.entries[i].kind === "assistant") {
        latest = data.entries[i].text;
        break;
      }
    }
    const ticks = toolTicks(data.entries);
    body = (
      <>
        {/* `size-auto`: the response is `size-full` by default and would push the ticks below the tile. */}
        {latest && <MessageResponse className="size-auto">{latest}</MessageResponse>}
        {ticks.length > 0 && (
          <ul className="m-0 mt-2 list-none p-0" aria-label="Recent tool calls">
            {ticks.map((tick, i) => (
              <li
                key={i}
                className="flex items-center gap-2 border-t border-widget-sep py-1 text-notch-w-meta"
              >
                {tick.isError ? (
                  <XIcon data-w-role="move" className="size-3.5 shrink-0 text-widget-acc-ink" aria-hidden />
                ) : (
                  <CheckIcon className="size-3.5 shrink-0 text-widget-ink-2" aria-hidden />
                )}
                <span className="min-w-0 flex-1 truncate" title={tick.title}>
                  {tick.text}
                </span>
                {tick.count > 1 && <span className="shrink-0 text-widget-ink-2">×{tick.count}</span>}
                <span className="shrink-0 text-widget-ink-2">{tick.isError ? "failed" : "done"}</span>
              </li>
            ))}
          </ul>
        )}
      </>
    );
  }
  return (
    <WidgetFrame size="large" label="Activity">
      <div className="h-full overflow-y-auto overscroll-contain p-3.5 text-notch-w-body leading-snug">{body}</div>
    </WidgetFrame>
  );
}

/* ---------------------------------------------------------------------------
 * Agents › Usage
 * ------------------------------------------------------------------------- */

/**
 * Agents › Usage in the widgets look: Screen Time. The shortest window leads as
 * the big numeral, and the plan's windows are the bars in Screen Time's blue,
 * cyan and orange. Same opt-in as the calm view: macOS asks before Claude
 * Code's sign-in is read, so nothing is fetched until the user says so.
 */
export function WidgetUsage() {
  const { quota, error, on, start } = useQuota();
  const bars = usageBars(quota);
  return (
    <UsageBoard
      idle={!on && !quota}
      error={error}
      loaded={quota !== null}
      bars={bars}
      extra={extraUsage(quota)}
      onStart={start}
    />
  );
}

export function UsageBoard({
  idle,
  error,
  loaded,
  bars,
  extra,
  onStart,
}: {
  idle: boolean;
  error: string | null;
  loaded: boolean;
  bars: UsageBar[];
  extra: number | null;
  onStart: () => void;
}) {
  // A lone tile sits in the middle columns rather than hanging off the left edge.
  const centred = "col-start-2";
  if (idle) {
    return (
      <WidgetBoard label="Plan usage">
        <WidgetFrame size="medium" label="Plan usage" className={centred}>
          <Idle icon={<BotIcon strokeWidth={1.6} />} title="Claude's plan limits">
            Read from Claude Code&apos;s sign-in. macOS asks once before Aloud can read it.
            <span className="mt-2 flex flex-wrap items-center gap-2">
              <Pill onClick={onStart}>Show plan usage</Pill>
              {error && <span>{error}</span>}
            </span>
          </Idle>
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  if (bars.length === 0) {
    return (
      <WidgetBoard label="Plan usage">
        <WidgetFrame size="medium" label="Plan usage" className={centred}>
          <Idle icon={<BotIcon strokeWidth={1.6} />} title={error ?? (loaded ? "No usage data" : "Reading…")} />
        </WidgetFrame>
      </WidgetBoard>
    );
  }
  const [lead] = bars;
  return (
    <WidgetBoard label="Plan usage">
      <WidgetFrame size="large" label="Plan usage" className={centred}>
        <div className="flex h-full flex-col p-3.5">
          <div className="flex items-center gap-2.5">
            <span
              aria-hidden
              className="grid size-7 shrink-0 place-items-center rounded-full bg-widget-tile-2 text-widget-ink-2"
            >
              <BotIcon className="size-4" strokeWidth={1.8} />
            </span>
            <div data-w-role={lead.role}>
              <Numeral text={`${lead.percent}%`} size="s" />
            </div>
            <span className="min-w-0 truncate text-notch-w-meta leading-tight font-medium text-widget-ink-2">
              of the {lead.label.toLowerCase()}
              {lead.resets && <br />}
              {lead.resets}
            </span>
          </div>

          {/* The chart: gridlines at 0, 50 and 100 as Screen Time's 0m/30m/60m. */}
          <div className="relative mt-4 mb-4 flex-1">
            {["100%", "50%", "0%"].map((tick, i) => (
              <div
                key={tick}
                className="absolute inset-x-0 flex -translate-y-1/2 items-center gap-1.5"
                // Evenly spaced; the arithmetic is the layout, not a value to tokenise.
                style={{ top: `${i * 50}%` }}
              >
                <span className="h-px flex-1 bg-widget-sep" />
                <span className="w-7 -translate-y-px text-right text-notch-w-over text-widget-ink-2 tabular-nums">
                  {tick}
                </span>
              </div>
            ))}
            <div className="absolute inset-y-0 right-9 left-2 flex items-end justify-around">
              {bars.map((b) => (
                <div
                  key={b.label}
                  role="meter"
                  aria-label={b.label}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={b.percent}
                  aria-valuetext={`${b.percent}%${b.warning ? `, ${b.warning}` : ""}`}
                  data-w-role={b.role}
                  className="w-9 rounded-t-notch-chip bg-widget-acc transition-[height] duration-[var(--spring-dur)] ease-[var(--spring)]"
                  // The bar's height is the reading itself.
                  style={{ height: `${b.value * 100}%`, minHeight: b.percent > 0 ? 3 : 0 }}
                />
              ))}
            </div>
          </div>

          <ul className="m-0 grid list-none grid-flow-col auto-cols-fr gap-2 p-0">
            {bars.map((b) => (
              <UsageColumn key={b.label} bar={b} />
            ))}
            {extra !== null && (
              <li data-w-role="move" className="min-w-0">
                <div className="truncate text-notch-w-meta font-medium text-widget-acc-ink">Extra</div>
                <div className="text-notch-w-headline leading-tight font-semibold">{extra}%</div>
                <div className="truncate text-notch-w-meta text-widget-ink-2">Past limits</div>
              </li>
            )}
          </ul>
        </div>
      </WidgetFrame>
    </WidgetBoard>
  );
}

/** Screen Time's column under the chart: a category in its colour, its amount, a line of context. */
function UsageColumn({ bar }: { bar: UsageBar }) {
  return (
    <li data-w-role={bar.role} className="min-w-0">
      <div className="truncate text-notch-w-meta font-medium text-widget-acc-ink">{bar.short}</div>
      <div className="text-notch-w-headline leading-tight font-semibold tabular-nums">{bar.percent}%</div>
      <div className="truncate text-notch-w-meta text-widget-ink-2">{bar.warning ?? bar.resets ?? " "}</div>
    </li>
  );
}
