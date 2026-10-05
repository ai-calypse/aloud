import { useState, type RefObject } from "react";
import {
  BotIcon,
  CheckIcon,
  CopyIcon,
  LoaderCircleIcon,
  Maximize2Icon,
  MicIcon,
  Trash2Icon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import {
  commands,
  type AgentPermission,
  type AgentSession,
  type AgentTranscript,
  type ApprovalReply,
  type Entry,
  type TimeSavedView,
} from "@/lib/bindings";
import { groupSteps, pendingStep, stepStatus, withDecision, type Run, type Step } from "@/lib/agent-steps";
import { agentHue, agentStage, stageSeed, type AgentStage } from "@/lib/agent-stage";
import { abbreviateCount, formatCost, tokenSummary } from "@/lib/format";
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
import { PermissionCard, baseName } from "@/features/overlay/PermissionCards";
import { useNotch } from "../../context";
import { justFinished, liveRun, useTimeSaved, voiceRow, voiceStats, type VoiceRow } from "../../voice-runs";
import {
  plainSummary,
  runSummary,
  savedDuration,
  savedShare,
  sessionLabel,
  sessionState,
  stepRole,
} from "./agents-logic";
import { Idle, Pill } from "./agents-parts";

/** The word on a row or a tile: in its accent's ink when it has one, quiet when it does not. */
const wordClass = (role: WidgetRole | undefined) => (role ? "text-widget-acc-ink" : "text-widget-ink-2");

/* ---------------------------------------------------------------------------
 * Agents › Voice
 * ------------------------------------------------------------------------- */

/**
 * Agents › Voice as a widget board: the command running now as a board (the
 * command and its steps in one Large tile, then either the approval it is
 * waiting on or its answer, how far it got and where it stands), and under it
 * the past runs, newest first, in a Large tile with two small ones counting
 * today and what failed.
 */
export function WidgetVoice() {
  const { agent, setAgent, runs, removeRun, bodyRef } = useNotch();
  const saved = useTimeSaved(runs);
  return (
    <VoiceBoards
      run={agent}
      runs={runs}
      saved={saved}
      bodyRef={bodyRef}
      onDecide={(confirmed) =>
        setAgent((r) => r && { ...r, steps: withDecision(r.steps, confirmed) })
      }
      onDelete={removeRun}
    />
  );
}

export function VoiceBoards({
  run,
  runs,
  bodyRef,
  onDecide,
  onDelete,
  now,
  saved,
}: {
  run: Run | null;
  runs: Entry[];
  /** Time saved, today and all time; the tile is left out until it has been read. */
  saved?: TimeSavedView | null;
  bodyRef: RefObject<HTMLDivElement | null>;
  onDecide: (confirmed: boolean) => void;
  onDelete: (id: number) => void;
  now?: number;
}) {
  const live = liveRun(run, runs);
  const [picked, setPicked] = useState<number | null | undefined>(undefined);
  const open = picked === undefined ? justFinished(run, runs) : picked;
  const stats = voiceStats(runs, now);
  return (
    <>
      {live && <VoiceRun key={live.heard} run={live} bodyRef={bodyRef} onDecide={onDecide} />}
      {runs.length === 0 ? (
        !live && (
          <WidgetBoard label="Voice history">
            <WidgetFrame size="medium" label="Voice history">
              <Idle icon={<MicIcon strokeWidth={1.6} />} title="Nothing said yet">
                What you say to Aloud, by dictation or as a command, is listed here, newest first.
                Hold the Command key and say what to do.
              </Idle>
            </WidgetFrame>
          </WidgetBoard>
        )
      ) : (
        <WidgetBoard label="Past voice runs" className={live ? "mt-3" : undefined}>
          <WidgetFrame size="large" label={live ? "Before this" : "Recent"}>
            <ul className="m-0 h-full list-none overflow-y-auto overscroll-contain p-3 pt-1.5">
              {runs.map((entry) => (
                <PastRun
                  key={entry.id}
                  row={voiceRow(entry, now)}
                  open={open === entry.id}
                  onToggle={() => setPicked(open === entry.id ? null : entry.id)}
                  onDelete={() => onDelete(entry.id)}
                />
              ))}
            </ul>
          </WidgetFrame>
          <WidgetFrame size="small" label="Today" ground="deep">
            <div className="flex h-full flex-col justify-between p-3.5">
              <MicIcon aria-hidden className="size-6 text-widget-ink-2" strokeWidth={1.6} />
              <div>
                <Numeral text={String(stats.today)} size="s" />
                <div className="text-notch-w-title font-medium text-widget-ink-2">
                  {stats.today === 1 ? "run today" : "runs today"}
                </div>
              </div>
            </div>
          </WidgetFrame>
          <WidgetFrame size="small" label="Failed" accent={stats.failed > 0 ? "move" : undefined}>
            <div className="flex h-full flex-col justify-between p-3.5">
              <TriangleAlertIcon
                aria-hidden
                className={cn("size-6", stats.failed > 0 ? "text-widget-acc-ink" : "text-widget-ink-2")}
                strokeWidth={1.6}
              />
              <div>
                <Numeral text={String(stats.failed)} size="s" className={stats.failed > 0 ? "text-widget-acc-ink" : undefined} />
                <div className="text-notch-w-title font-medium text-widget-ink-2">
                  {stats.failed === 1 ? "failed command" : "failed commands"}
                </div>
              </div>
            </div>
          </WidgetFrame>
          {saved && <TimeSavedTile saved={saved} />}
        </WidgetBoard>
      )}
    </>
  );
}

/**
 * What Aloud has given back: dictating instead of typing, and commands instead
 * of doing the task by hand, as one total with its two parts. Both are
 * estimates (`aloud_store::stats` states the typing speed and the minutes an
 * action is assumed to save), so the tile says "about".
 */
function TimeSavedTile({ saved }: { saved: TimeSavedView }) {
  const { allTime, today } = saved;
  return (
    <WidgetFrame size="medium" label="Time saved" accent="blue">
      <div className="flex h-full flex-col justify-between p-3.5">
        <div className="flex items-start justify-between gap-3">
          <div>
            <Numeral text={savedDuration(allTime.totalSeconds)} size="s" className="text-widget-acc-ink" />
            <div className="mt-1 text-notch-w-title font-medium text-widget-ink-2">saved in all, about</div>
          </div>
          <div className="text-right">
            <div className="text-notch-w-headline leading-tight font-semibold tabular-nums">
              {savedDuration(today.totalSeconds)}
            </div>
            <div className="text-notch-w-meta font-medium text-widget-ink-2">today</div>
          </div>
        </div>
        <div>
          <div
            role="img"
            aria-label={`${savedDuration(allTime.dictationSeconds)} from dictating, ${savedDuration(allTime.actionSeconds)} from actions`}
            className="mb-2 flex h-1.5 w-full gap-0.5 overflow-hidden rounded-full"
          >
            {(
              [
                ["blue", allTime.dictationSeconds],
                ["agent", allTime.actionSeconds],
              ] as const
            ).map(([role, part]) => (
              <span
                key={role}
                data-w-role={role}
                className="rounded-full bg-widget-acc"
                style={{ flexGrow: savedShare(part, allTime.totalSeconds), display: part ? undefined : "none" }}
              />
            ))}
          </div>
          <div className="flex justify-between gap-3 text-notch-w-meta font-medium">
            <span className="flex items-center gap-1.5">
              <MicIcon data-w-role="blue" aria-hidden className="size-3.5 text-widget-acc-ink" strokeWidth={2} />
              {savedDuration(allTime.dictationSeconds)} dictating
            </span>
            <span className="flex items-center gap-1.5">
              <BotIcon data-w-role="agent" aria-hidden className="size-3.5 text-widget-acc-ink" strokeWidth={2} />
              {savedDuration(allTime.actionSeconds)} · {allTime.actions} {allTime.actions === 1 ? "action" : "actions"}
            </span>
          </div>
        </div>
      </div>
    </WidgetFrame>
  );
}

/** One past run in the Recent tile: two lines closed, the whole text when open. */
function PastRun({
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
  const Glyph = row.kind === "dictation" ? MicIcon : BotIcon;
  return (
    <li className="border-b border-widget-sep last:border-b-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full items-center gap-2 rounded-notch-chip py-1.5 text-left outline-none focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-notch-focus"
      >
        <Glyph aria-hidden className="size-4 shrink-0 text-widget-ink-2" strokeWidth={1.8} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-notch-w-title font-medium">{row.said || "(nothing heard)"}</span>
          <span
            data-w-role={row.failed ? "move" : undefined}
            className={cn("block truncate text-notch-w-meta", wordClass(row.failed ? "move" : undefined))}
          >
            {row.result}
          </span>
        </span>
        <span className="shrink-0 text-notch-w-meta text-widget-ink-2 tabular-nums">{row.when}</span>
      </button>
      {open && (
        <div className="pb-2 pl-6">
          {row.app && <p className="m-0 text-notch-w-meta text-widget-ink-2">in {row.app}</p>}
          <p className="m-0 mt-0.5 text-notch-w-body leading-snug whitespace-pre-wrap select-text">
            {row.full || row.result}
          </p>
          <div className="mt-1.5 flex gap-1.5">
            <CopyPill text={row.full || row.said} />
            <Pill quiet onClick={onDelete}>
              <Trash2Icon className="size-3.5" aria-hidden />
              Delete
            </Pill>
          </div>
        </div>
      )}
    </li>
  );
}

export function VoiceRun({
  run,
  bodyRef,
  onExpand,
  onDecide,
}: {
  run: Run;
  bodyRef?: RefObject<HTMLDivElement | null>;
  /** Given on the compact panel, which has no sections to browse to. */
  onExpand?: () => void;
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
            {onExpand && (
              <Pill quiet onClick={onExpand}>
                <Maximize2Icon className="size-3.5" strokeWidth={2.5} aria-hidden />
                Open in Agents
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
 * The approval card the first waiting request needs, and a session's tile and
 * detail board: what Agents › Coding opens a session into. One card at a time:
 * every permission card answers Enter for itself, so two on screen would
 * approve both.
 */

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
  // Two figures a glance can use; the full breakdown (with cache reads) is the tooltip.
  const usage =
    data && data.entries.length > 0
      ? [
          data.costUsd != null && data.costUsd > 0 ? formatCost(data.costUsd) : null,
          `${abbreviateCount(data.inputTokens + data.outputTokens)} tokens`,
        ]
          .filter(Boolean)
          .join(" · ")
      : null;
  const usageDetail = data ? tokenSummary(data.inputTokens, data.cacheReadTokens, data.outputTokens) : undefined;
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
              <span
                className="line-clamp-2 text-notch-w-meta leading-snug text-widget-ink-2"
                title={[session.cwd, usageDetail].filter(Boolean).join("\n") || undefined}
              >
                {[folder, usage].filter(Boolean).join(" · ")}
              </span>
            )
          }
        />
        {session.subagents.length > 0 && <SubagentTile subagents={session.subagents} />}
      </WidgetBoard>
    </>
  );
}

/** Each subagent as a line: whether it is working, what it is, and what it is doing now. */
function SubagentTile({ subagents }: { subagents: AgentSession["subagents"] }) {
  const running = subagents.filter((sub) => sub.busy).length;
  return (
    <WidgetFrame size="medium" label={running > 0 ? `Subagents · ${running} running` : `Subagents · ${subagents.length} done`}>
      <ul className="m-0 h-full list-none overflow-y-auto overscroll-contain p-3.5 py-2.5">
        {subagents.map((sub, i) => (
          <li key={i} className="flex items-center gap-2.5 border-b border-widget-sep py-1.5 last:border-b-0">
            {sub.busy ? (
              <LoaderCircleIcon data-w-role="agent" className="size-4 shrink-0 animate-spin text-widget-acc-ink" aria-hidden />
            ) : (
              <CheckIcon className="size-4 shrink-0 text-widget-ink-2" aria-hidden />
            )}
            <span className="min-w-0 flex-1">
              <span className="block truncate text-notch-w-title font-medium">{sub.label}</span>
              <span className="block truncate text-notch-w-meta text-widget-ink-2">
                {[sub.model, sub.busy ? sub.tool : null].filter(Boolean).join(" · ")}
              </span>
            </span>
            <span
              data-w-role={sub.busy ? "agent" : undefined}
              className={cn("shrink-0 text-notch-w-meta font-medium", wordClass(sub.busy ? "agent" : undefined))}
            >
              {sub.busy ? "Running" : "Done"}
            </span>
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

/**
 * What the agent just did, as a glance: the opening of its latest message, the
 * count of tool calls it made, and the last few of them. A reply is often a
 * page of markdown, which in a tile is a wall, so the whole of it is behind
 * "Read all" rather than the default.
 */
function ActivityTile({ data, error }: { data: AgentTranscript | null; error: string | null }) {
  const [full, setFull] = useState(false);
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
    const calls = data.entries.filter((e) => e.kind === "toolUse");
    const failed = calls.filter((e) => e.isError).length;
    const ticks = toolTicks(data.entries);
    const summary = latest ? plainSummary(latest) : "";
    // Whether there is more than the summary shows: a longer text, or structure it dropped.
    const more = latest !== null && summary !== latest.trim();
    body = full && latest ? (
      <>
        {/* `size-auto`: the response is `size-full` by default and would push the button below the tile. */}
        <MessageResponse className="size-auto">{latest}</MessageResponse>
        <Pill quiet className="mt-2" onClick={() => setFull(false)}>
          Show summary
        </Pill>
      </>
    ) : (
      <>
        {summary && (
          <p className="m-0 line-clamp-4 text-notch-w-body leading-snug text-pretty select-text">{summary}</p>
        )}
        {more && (
          <Pill quiet className="mt-2" onClick={() => setFull(true)}>
            Read all
          </Pill>
        )}
        {calls.length > 0 && (
          <p className="m-0 mt-3 mb-1 text-notch-w-meta font-medium text-widget-ink-2">
            {calls.length} tool {calls.length === 1 ? "call" : "calls"}
            {failed > 0 && (
              <span data-w-role="move" className="text-widget-acc-ink">
                {" · "}
                {failed} failed
              </span>
            )}
          </p>
        )}
        {ticks.length > 0 && (
          <ul className="m-0 list-none p-0" aria-label="Recent tool calls">
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
