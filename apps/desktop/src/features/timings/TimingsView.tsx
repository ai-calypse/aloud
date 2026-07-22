import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, events, type Span } from "@/lib/bindings";
import { covered, percentile, spanFields as fields } from "@/lib/waterfall";
import { cn } from "@/lib/utils";

/** Runs fetched for the aggregates. The waterfall renders far fewer. */
const FETCH = 500;
const WATERFALLS = 10;

/** Tokens, not hexes: these bars sit on a card that flips with the OS, and a
 *  frozen light-mode palette washes out on the dark one. */
const STAGE_TINT: Record<string, string> = {
  finalize_audio: "var(--sys-orange)",
  transcribe: "var(--sys-blue)",
  cleanup: "var(--sys-indigo)",
  llm_fallback: "var(--sys-pink)",
  paste: "var(--sys-green)",
  transform: "var(--sys-indigo)",
  ask_answer: "var(--sys-teal)",
  // Agent mode.
  agent_run: "var(--sys-grey)",
  tool_discovery: "var(--sys-teal)",
  tool_select: "var(--sys-indigo)",
  tool_select_retry: "var(--sys-pink)",
  composio_exec: "var(--sys-purple)",
  // Human time, not machine time — greyed so it never reads as a hot stage.
  confirm: "var(--text3)",
};

/** Any stage the map does not name. */
const STAGE_TINT_FALLBACK = "var(--sys-grey)";

type Utterance = {
  id: number;
  mode: string;
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
      if (!root) return null;
      return {
        id,
        // Runs predating mode tagging are still real runs — bucketed, not lost.
        mode: String(fields(root).mode ?? "untagged"),
        root,
        stages: spans.filter((s) => s !== root),
      };
    })
    .filter((u): u is Utterance => u !== null)
    .sort((a, b) => b.id - a.id);
}

const mean = (values: number[]) =>
  values.length === 0
    ? 0
    : Math.round(values.reduce((a, b) => a + b, 0) / values.length);

/** Per-stage aggregates within one mode, slowest first. */
function stageStats(runs: Utterance[]) {
  const byStage = new Map<string, number[]>();
  for (const run of runs) {
    for (const s of run.stages) {
      const list = byStage.get(s.stage);
      if (list) list.push(s.durationMs);
      else byStage.set(s.stage, [s.durationMs]);
    }
  }
  return [...byStage.entries()]
    .map(([stage, ms]) => ({
      stage,
      n: ms.length,
      avg: mean(ms),
      p95: percentile(ms, 95),
    }))
    .sort((a, b) => b.avg - a.avg);
}

export function TimingsView() {
  const [all, setAll] = useState<Utterance[]>([]);
  const [mode, setMode] = useState<string>("all");

  const refresh = useCallback(() => {
    commands.recentTimings(FETCH).then((result) => {
      if (result.status === "ok") setAll(group(result.data));
    });
  }, []);

  useEffect(() => {
    refresh();
    // Every terminal phase, not just "done": agent and ask runs finish on
    // Idle. The row is written when the span closes, just after the phase is
    // emitted — hence the delay.
    const unlisten = events.dictationPhase.listen((event) => {
      if (["done", "error", "idle"].includes(event.payload)) {
        setTimeout(refresh, 150);
      }
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, [refresh]);

  const modes = useMemo(
    () => [...new Set(all.map((u) => u.mode))].sort(),
    [all],
  );
  const runs = useMemo(
    () => (mode === "all" ? all : all.filter((u) => u.mode === mode)),
    [all, mode],
  );
  const stats = useMemo(() => stageStats(runs), [runs]);

  if (all.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No runs recorded yet. Dictate once and the breakdown appears here.
      </p>
    );
  }

  const totals = runs.map((u) => u.root.durationMs);
  const slowestStage = Math.max(...stats.map((s) => s.p95), 1);
  // Oldest to newest, so the sparkline reads left to right like a timeline.
  const overTime = [...runs].reverse();
  const peak = Math.max(...totals, 1);

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap gap-1.5">
        {["all", ...modes].map((m) => {
          const n =
            m === "all" ? all.length : all.filter((u) => u.mode === m).length;
          return (
            <button
              key={m}
              onClick={() => setMode(m)}
              className={cn(
                "rounded-[7px] px-2.5 py-1 text-[12px] font-medium transition-colors",
                mode === m
                  ? "bg-brand text-white"
                  : "bg-field hover:bg-surface",
              )}
            >
              {m} <span className="opacity-60">{n}</span>
            </button>
          );
        })}
      </div>

      <section className="rounded-card border-[0.5px] border-surface-border bg-surface p-5 shadow-card">
        <div className="flex gap-8">
          {[
            { label: "runs", value: runs.length },
            { label: "avg", value: `${mean(totals)} ms` },
            { label: "p50", value: `${percentile(totals, 50)} ms` },
            { label: "p95", value: `${percentile(totals, 95)} ms` },
          ].map((s) => (
            <div key={s.label}>
              <div className="text-[11px] uppercase tracking-wide text-ink-2">
                {s.label}
              </div>
              <div className="text-lg font-semibold tabular-nums">
                {s.value}
              </div>
            </div>
          ))}
        </div>

        <div className="mt-5 flex h-16 items-end gap-px">
          {overTime.map((u) => (
            <div
              key={u.id}
              className="min-w-[2px] flex-1 rounded-t-[2px]"
              style={{
                height: `${Math.max((u.root.durationMs / peak) * 100, 2)}%`,
                background: u.root.success ? "var(--accent)" : "var(--warning)",
              }}
              title={`run ${u.id} · ${u.mode} · ${u.root.durationMs} ms`}
            />
          ))}
        </div>
        <div className="mt-1 flex justify-between text-[11px] text-ink-2">
          <span>oldest</span>
          <span>{overTime.length} runs</span>
          <span>newest</span>
        </div>
      </section>

      {/* Sorted on p95, not avg: a stage that is usually fast and occasionally
          terrible averages out to fine. */}
      <section className="rounded-card border-[0.5px] border-surface-border bg-surface p-5 shadow-card">
        <h2 className="mb-3 text-[13px] font-semibold">Stages · {mode}</h2>
        <div className="grid grid-cols-[130px_1fr_46px_52px_36px] items-center gap-2 text-[11px] text-ink-2">
          <span>stage</span>
          <span />
          <span className="text-right">avg</span>
          <span className="text-right">p95</span>
          <span className="text-right">n</span>
        </div>
        {stats.map((s) => (
          <div
            key={s.stage}
            className="mt-1.5 grid grid-cols-[130px_1fr_46px_52px_36px] items-center gap-2 text-[11px]"
          >
            <span className="truncate">{s.stage}</span>
            <div className="h-3 rounded-[3px] bg-field">
              <div
                className="h-full rounded-[3px] opacity-40"
                style={{
                  width: `${(s.p95 / slowestStage) * 100}%`,
                  background: STAGE_TINT[s.stage] ?? STAGE_TINT_FALLBACK,
                }}
              >
                <div
                  className="h-full rounded-[3px]"
                  style={{
                    width: `${(s.avg / Math.max(s.p95, 1)) * 100}%`,
                    background: STAGE_TINT[s.stage] ?? STAGE_TINT_FALLBACK,
                  }}
                />
              </div>
            </div>
            <span className="text-right tabular-nums">{s.avg}</span>
            <span className="text-right tabular-nums text-ink-2">{s.p95}</span>
            <span className="text-right tabular-nums text-ink-2">{s.n}</span>
          </div>
        ))}
      </section>

      <h2 className="text-[13px] font-semibold">Recent runs</h2>
      {runs.slice(0, WATERFALLS).map((u) => {
        const total = Math.max(u.root.durationMs, 1);
        const f = fields(u.root);
        // Confirmation time is the user deciding; counted as pipeline latency
        // it makes every approved write look like a catastrophic regression.
        const waiting = u.stages
          .filter((s) => s.stage === "confirm")
          .reduce((n, s) => n + s.durationMs, 0);
        return (
          <section
            key={u.id}
            className="rounded-card border-[0.5px] border-surface-border bg-surface p-4 shadow-card"
          >
            <header className="mb-3 flex items-baseline gap-2 text-[13px]">
              <span
                className={cn(
                  "text-base font-semibold tabular-nums",
                  !u.root.success && "text-warning",
                )}
              >
                {total} ms
              </span>
              <span className="text-ink-2">
                {u.mode}
                {f.outcome ? ` · ${f.outcome}` : ""}
                {typeof f.spoken_ms === "number" &&
                  ` · ${f.spoken_ms} ms spoken`}
                {waiting > 0 && ` · ${waiting} ms waiting on you`}
              </span>
            </header>

            <div className="flex flex-col gap-1">
              {u.stages.map((s, i) => {
                // Offset within the utterance: the bar's position is when the
                // stage ran, its width how long it took.
                const offset = Math.max(s.startedAt - u.root.startedAt, 0);
                return (
                  <div
                    key={`${s.stage}-${i}`}
                    className="grid grid-cols-[130px_1fr_58px] items-center gap-2 text-[11px]"
                  >
                    <span className="truncate text-ink-2">{s.stage}</span>
                    <div className="h-3 rounded-[3px] bg-field">
                      <div
                        className="h-full rounded-[3px]"
                        style={{
                          marginLeft: `${(offset / total) * 100}%`,
                          // Sub-millisecond stages still need to be visible.
                          width: `${Math.max((s.durationMs / total) * 100, 1)}%`,
                          background: s.success
                            ? (STAGE_TINT[s.stage] ?? STAGE_TINT_FALLBACK)
                            : "var(--warning)",
                        }}
                        title={s.fields}
                      />
                    </div>
                    <span className="text-right tabular-nums text-ink-2">
                      {s.durationMs} ms
                    </span>
                  </div>
                );
              })}
              {/* Stages never sum to the total; the gap is the app's own
                  overhead. */}
              <div className="mt-1 grid grid-cols-[130px_1fr_58px] gap-2 text-[11px] text-ink-2">
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
