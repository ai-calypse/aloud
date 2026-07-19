import { useCallback, useEffect, useState } from "react";
import { commands, events, type Span } from "@/lib/bindings";
import { cn } from "@/lib/utils";

const UTTERANCES = 20;

/** One bar per stage, keyed to the stage name so a run is readable at a glance. */
const STAGE_TINT: Record<string, string> = {
  finalize_audio: "#ff9f0a",
  transcribe: "#007aff",
  cleanup: "#5856d6",
  llm_fallback: "#ff375f",
  paste: "#34c759",
  // Agent mode.
  agent_run: "#8e8e93",
  tool_discovery: "#64d2ff",
  tool_select: "#5856d6",
  tool_select_retry: "#ff375f",
  composio_exec: "#af52de",
  // Human time, not machine time — greyed so it never reads as a hot stage.
  confirm: "#c7c7cc",
};

type Utterance = {
  id: number;
  root: Span;
  /** Every stage except the root, which is the track the others sit on. */
  stages: Span[];
};

/** Rows arrive flat and newest-first; the waterfall needs them grouped. */
function group(rows: Span[]): Utterance[] {
  const byId = new Map<number, Span[]>();
  for (const row of rows) {
    const list = byId.get(row.utteranceId);
    if (list) list.push(row);
    else byId.set(row.utteranceId, [row]);
  }
  return [...byId.entries()]
    .map(([id, spans]) => {
      const root = spans.find((s) => s.stage === "utterance");
      return root
        ? { id, root, stages: spans.filter((s) => s !== root) }
        : null;
    })
    .filter((u): u is Utterance => u !== null)
    .sort((a, b) => b.id - a.id);
}

/**
 * Wall-clock the stages actually cover. Spans nest — `llm_fallback` runs
 * inside `cleanup` — so summing durations double-counts and makes the
 * unaccounted remainder come out negative.
 */
function covered(stages: Span[]): number {
  const spans = [...stages].sort((a, b) => a.startedAt - b.startedAt);
  let total = 0;
  let end = -Infinity;
  for (const s of spans) {
    const start = Math.max(s.startedAt, end);
    end = Math.max(end, s.startedAt + s.durationMs);
    total += Math.max(end - start, 0);
  }
  return total;
}

function fields(span: Span): Record<string, unknown> {
  try {
    return JSON.parse(span.fields);
  } catch {
    return {};
  }
}

export function TimingsView() {
  const [utterances, setUtterances] = useState<Utterance[]>([]);

  const refresh = useCallback(() => {
    commands.recentTimings(UTTERANCES).then((result) => {
      if (result.status === "ok") setUtterances(group(result.data));
    });
  }, []);

  useEffect(() => {
    refresh();
    // Every terminal phase, not just "done": agent and ask runs finish on
    // Idle, so watching done/error alone missed exactly the runs worth
    // inspecting. The row is written when the span closes, which is just
    // after the phase is emitted — hence the delay.
    const unlisten = events.dictationPhase.listen((event) => {
      if (["done", "error", "idle"].includes(event.payload)) {
        setTimeout(refresh, 150);
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, [refresh]);

  if (utterances.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No dictations recorded yet. Speak once and the waterfall appears here.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {utterances.map((u) => {
        const total = Math.max(u.root.durationMs, 1);
        const f = fields(u.root);
        // Time spent at a confirmation prompt is the user deciding. Counting
        // it as pipeline latency would make every approved write look like a
        // catastrophic regression.
        const waiting = u.stages
          .filter((s) => s.stage === "confirm")
          .reduce((n, s) => n + s.durationMs, 0);
        return (
          <section
            key={u.id}
            className="rounded-[11px] border-[0.5px] border-[var(--card-border)] bg-[var(--card-solid)] p-4 shadow-[0_1px_2px_rgba(0,0,0,.05)]"
          >
            <header className="mb-3 flex items-baseline gap-2 text-[13px]">
              <span
                className={cn(
                  "text-base font-semibold tabular-nums",
                  !u.root.success && "text-[var(--warning)]",
                )}
              >
                {total} ms
              </span>
              <span className="text-[var(--text2)]">
                {String(f.outcome ?? f.mode ?? "—")}
                {typeof f.spoken_ms === "number" &&
                  ` · ${f.spoken_ms} ms spoken`}
                {waiting > 0 && ` · ${waiting} ms waiting on you`}
              </span>
            </header>

            <div className="flex flex-col gap-1">
              {u.stages.map((s, i) => {
                // Offset within the utterance, so a bar's position shows when
                // the stage ran, not just how long it took.
                const offset = Math.max(s.startedAt - u.root.startedAt, 0);
                return (
                  <div
                    key={`${s.stage}-${i}`}
                    className="grid grid-cols-[110px_1fr_58px] items-center gap-2 text-[11px]"
                  >
                    <span className="truncate text-[var(--text2)]">
                      {s.stage}
                    </span>
                    <div className="h-3 rounded-[3px] bg-[var(--field)]">
                      <div
                        className="h-full rounded-[3px]"
                        style={{
                          marginLeft: `${(offset / total) * 100}%`,
                          // Sub-millisecond stages still need to be visible.
                          width: `${Math.max((s.durationMs / total) * 100, 1)}%`,
                          background: s.success
                            ? (STAGE_TINT[s.stage] ?? "#8e8e93")
                            : "var(--warning)",
                        }}
                        title={s.fields}
                      />
                    </div>
                    <span className="text-right tabular-nums text-[var(--text2)]">
                      {s.durationMs} ms
                    </span>
                  </div>
                );
              })}
              {/* Stages never sum to the total; the gap is the app's own
                  overhead, and naming it is the point of the panel. */}
              <div className="mt-1 grid grid-cols-[110px_1fr_58px] gap-2 text-[11px] text-[var(--text2)]">
                <span>unaccounted</span>
                <span />
                <span className="text-right tabular-nums">
                  {Math.max(total - covered(u.stages), 0)} ms
                </span>
              </div>
            </div>
          </section>
        );
      })}
    </div>
  );
}
