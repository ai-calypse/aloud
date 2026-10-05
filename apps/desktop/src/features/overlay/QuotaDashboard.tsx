import { useEffect, useState } from "react";
import { commands, type Quota, type QuotaWindow } from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { Label, Lead, List, Value } from "@/components/notch/list";
import { Bar } from "@/components/notch/meters";
import { UsageChart } from "./UsageChart";
import { countdown, elapsedFraction, extraSpend, pace, percent, shownWindows, tone, type ShownWindow } from "./usage-logic";

/**
 * The plan-quota dashboard — Agents › Usage: every limit Claude's usage
 * endpoint reports (the 5-hour window, the week overall and per model, the
 * non-model buckets, the extra-usage credit pool), each with its reset
 * countdown and how it runs against the clock, and the last day and week as
 * lines. Reads the Keychain token + usage API through `agent_quota`, polling on
 * an interval. Silent on error (not signed in / token expired) except for a
 * quiet one-line note.
 */
const QUOTA_POLL_MS = 60_000;

/**
 * The last reading, kept for Home's plan-usage meter. Home must not read the
 * token itself (that is the Keychain prompt this view makes opt-in), so it
 * shows what Usage already fetched, if that was recent.
 */
let lastQuota: { quota: Quota; at: number } | null = null;

export function recentQuota(maxAgeMs = 5 * 60_000): Quota | null {
  return lastQuota && Date.now() - lastQuota.at < maxAgeMs ? lastQuota.quota : null;
}

function useQuota() {
  const [quota, setQuota] = useState<Quota | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Opt-in: reading the token prompts for Keychain access, so nothing fetches
  // until the user asks. `on` also gates polling — an errored read must not loop
  // the prompt, so a failure turns polling back off.
  const [on, setOn] = useState(false);

  useEffect(() => {
    if (!on) return;
    let live = true;
    const load = () =>
      void commands.agentQuota().then((res) => {
        if (!live) return;
        if (res.status === "ok") {
          lastQuota = { quota: res.data, at: Date.now() };
          setQuota(res.data);
          setError(null);
        } else {
          setError(res.error);
          setOn(false); // stop polling so a rejected token can't re-prompt in a loop
        }
      });
    load();
    const timer = setInterval(load, QUOTA_POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [on]);

  return { quota, error, on, start: () => setOn(true) };
}

export function QuotaDashboard() {
  const { quota, error, on, start } = useQuota();

  // Idle: an explanation and a button, no fetch, no prompt until asked.
  if (!on && !quota) {
    return (
      <div className="flex flex-col items-start gap-2 font-notch">
        <p className="m-0 text-notch-body text-notch-ink-2">
          Claude's plan limits, read from Claude Code's sign-in. macOS asks once
          before Aloud can read it.
        </p>
        <Button variant="notch" size="notch" onClick={start}>
          Show plan usage
        </Button>
        {error && <p className="m-0 text-notch-meta text-notch-ink-3">{error}</p>}
      </div>
    );
  }

  const shown = shownWindows(quota);
  if (shown.length === 0) {
    return (
      <p className="m-0 font-notch text-notch-body text-notch-ink-3">
        {error ?? (quota ? "No usage data" : "Reading…")}
      </p>
    );
  }

  // The shortest window leads — it is the one that runs out mid-session; the
  // rest are rows, the weekly ones grouped by when they reset (they share a
  // reset, so it is said once) and the credit pool on its own.
  const [lead, ...rest] = shown;
  const now = Date.now();
  const groups = groupByReset(rest);
  const extra = quota?.extra;
  const spend = extra ? extraSpend(extra) : null;
  const extraPct = extra?.utilization ?? quota?.extraUsagePercent ?? null;
  const history = quota?.history ?? [];
  return (
    <div className="grid grid-cols-[1.1fr_1fr] gap-x-6 gap-y-4 font-notch">
      <div className="flex flex-col gap-4">
        <div>
          <Label>Claude · {lead.label}</Label>
          <Lead sub={subtitle(lead.window, lead.spanMs, now)}>{Math.round(percent(lead.window))}%</Lead>
          <WindowBar window={lead.window} spanMs={lead.spanMs} label={lead.label} now={now} className="mt-2.5" />
        </div>
        {quota?.fiveHour && (
          <UsageChart title="5-hour window, last 24 hours" history={history} pick={(s) => ({ value: s.fiveHour, resetsAt: s.fiveHourResetsAt })} spanMs={DAY} spanLabel="24 h ago" now={now} />
        )}
        {quota?.sevenDay && (
          <UsageChart title="Week, all models, last 7 days" history={history} pick={(s) => ({ value: s.sevenDay, resetsAt: s.sevenDayResetsAt })} spanMs={7 * DAY} spanLabel="7 d ago" now={now} />
        )}
      </div>
      <div className="flex flex-col gap-3">
        {groups.map((group) => (
          <section key={group.resetsAt ?? "none"}>
            <Label className="px-2">{groupLabel(group, now)}</Label>
            <List label={groupLabel(group, now)}>
              {group.windows.map(({ key, label, spanMs, window }) => (
                <MeterRow key={key} title={label.replace(/^Week · /, "")} value={percent(window) / 100}>
                  <WindowBar window={window} spanMs={spanMs} label={label} now={now} />
                </MeterRow>
              ))}
            </List>
          </section>
        ))}
        {extra && (
          <section>
            <Label className="px-2">Extra usage this month · past the plan's limits</Label>
            <List label="Extra usage">
              {extra.enabled && spend ? (
                <MeterRow title={`${spend.spent} of ${spend.limit}`} value={spend.fraction} text={extraPct}>
                  <Bar value={spend.fraction} tone={tone(spend.fraction * 100)} label="Extra usage this month" />
                </MeterRow>
              ) : (
                <MeterRow title={extra.enabled ? "On" : "Off"} />
              )}
            </List>
          </section>
        )}
        <p className="m-0 px-2 text-notch-label text-notch-ink-3">
          The tick on each bar is how far through its window the clock is. Charts join readings from one window; a reset starts a new line.
        </p>
      </div>
    </div>
  );
}

/** One meter on a line: its name, the bar, and the number, so a handful of windows stay a handful of lines. */
function MeterRow({
  title,
  value,
  text,
  children,
}: {
  title: string;
  value?: number;
  /** The number when it is not the bar's own value: the pool's utilisation, not spend over cap. */
  text?: number | null;
  children?: React.ReactNode;
}) {
  const shown = text ?? (value == null ? null : value * 100);
  return (
    <li className="flex min-h-[34px] items-center gap-3 border-b border-notch-sep px-2 py-1.5 last:border-b-0">
      <span className="min-w-0 flex-1 truncate text-notch-body text-notch-ink">{title}</span>
      {children && <span className="w-[38%] shrink-0">{children}</span>}
      {shown != null && <Value className="w-9 text-right">{Math.round(shown)}%</Value>}
    </li>
  );
}

type Group = { resetsAt: string | null; spanMs: number; windows: ShownWindow[] };

/** Windows that reset together, in the order the plan lists them; one without a reset time stands alone. */
function groupByReset(windows: ShownWindow[]): Group[] {
  const groups: Group[] = [];
  for (const w of windows) {
    // To the minute: the endpoint's fractional seconds differ between windows that reset together.
    const at = w.window.resetsAt ? Math.floor(Date.parse(w.window.resetsAt) / 60_000) : null;
    const into = groups.find((g) => g.resetsAt === (at == null ? null : String(at)));
    if (into) into.windows.push(w);
    else groups.push({ resetsAt: at == null ? null : String(at), spanMs: w.spanMs, windows: [w] });
  }
  return groups;
}

/** "Week · resets in 2 d 23 h · Under pace": what the group is, when it frees up, and how the first window runs. */
function groupLabel(group: Group, now: number): string {
  const first = group.windows[0];
  const week = group.spanMs >= 7 * DAY;
  return [week ? "Week" : first.label, first.window.resetsAt ? `resets in ${countdown(first.window.resetsAt, now)}` : null, paceOf(first.window, first.spanMs, now)]
    .filter(Boolean)
    .join(" · ");
}

const DAY = 86_400_000;

function paceOf(w: QuotaWindow, spanMs: number, now: number) {
  return pace(percent(w), elapsedFraction(w.resetsAt, spanMs, now));
}

/** "Resets in 2 h 14 m · Ahead of pace": when it frees up and whether use is running ahead of the clock. */
function subtitle(w: QuotaWindow, spanMs: number, now: number): string | undefined {
  return [w.resetsAt ? `Resets in ${countdown(w.resetsAt, now)}` : null, paceOf(w, spanMs, now)].filter(Boolean).join(" · ") || undefined;
}

/** A window's meter with a tick where the clock stands, so the fill can be read against it. */
function WindowBar({
  window: w,
  spanMs,
  label,
  now,
  className,
}: {
  window: QuotaWindow;
  spanMs: number;
  label: string;
  now: number;
  className?: string;
}) {
  const elapsed = elapsedFraction(w.resetsAt, spanMs, now);
  return (
    <div className={`relative ${className ?? ""}`}>
      <Bar value={percent(w) / 100} tone={tone(percent(w))} label={label} />
      {elapsed != null && (
        <span
          aria-hidden
          className="absolute top-1/2 h-3 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-notch-ink-3"
          style={{ left: `${elapsed * 100}%` }}
        />
      )}
    </div>
  );
}
