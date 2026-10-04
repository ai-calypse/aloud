import { useEffect, useState } from "react";
import { commands, type Quota, type QuotaWindow } from "@/lib/bindings";
import { resetsIn } from "@/lib/format";
import { Button } from "@/components/ui/button";
import { Label, Lead, List, Row, Value } from "@/components/notch/list";
import { Bar, type Tone } from "@/components/notch/meters";

/**
 * The plan-quota dashboard — Agents › Usage: Claude's rolling usage limits.
 * Reads the Keychain token + usage API through `agent_quota`, polling on an
 * interval. Silent on error (not signed in / token expired) except for a quiet
 * one-line note.
 */
const QUOTA_POLL_MS = 60_000;

export function QuotaDashboard() {
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

  // Idle: an explanation and a button, no fetch, no prompt until asked.
  if (!on && !quota) {
    return (
      <div className="flex flex-col items-start gap-2 font-notch">
        <p className="m-0 text-notch-body text-notch-ink-2">
          Claude's plan limits, read from Claude Code's sign-in. macOS asks once
          before Aloud can read it.
        </p>
        <Button variant="notch" size="notch" onClick={() => setOn(true)}>
          Show plan usage
        </Button>
        {error && <p className="m-0 text-notch-meta text-notch-ink-3">{error}</p>}
      </div>
    );
  }

  const windows: [string, QuotaWindow | null][] = quota
    ? [
        ["5-hour window", quota.fiveHour],
        ["Week", quota.sevenDay],
        ["Week · Opus", quota.sevenDayOpus],
      ]
    : [];
  const shown = windows.filter((entry): entry is [string, QuotaWindow] => entry[1] != null);
  if (shown.length === 0) {
    return (
      <p className="m-0 font-notch text-notch-body text-notch-ink-3">
        {error ?? (quota ? "No usage data" : "Reading…")}
      </p>
    );
  }

  // The shortest window leads — it is the one that runs out mid-session; the
  // rest are rows.
  const [lead, ...rest] = shown;
  const [leadLabel, leadWindow] = lead;
  return (
    <div className="grid grid-cols-[1.15fr_1fr] gap-5 font-notch">
      <div>
        <Label>Claude · {leadLabel}</Label>
        <Lead sub={leadWindow.resetsAt ? `Resets in ${resetsIn(leadWindow.resetsAt)}` : undefined}>
          {Math.round(percent(leadWindow))}%
        </Lead>
        <Bar className="mt-2.5" value={percent(leadWindow) / 100} tone={tone(leadWindow)} label={leadLabel} />
      </div>
      <List label="Other limits">
        {rest.map(([label, w]) => (
          <Row
            key={label}
            title={label}
            subtitle={w.resetsAt ? `Resets in ${resetsIn(w.resetsAt)}` : undefined}
            trailing={<Value>{Math.round(percent(w))}%</Value>}
          />
        ))}
        {quota?.extraUsagePercent != null && quota.extraUsagePercent > 0 && (
          <Row
            title="Extra usage"
            subtitle="Past the plan's limits"
            trailing={<Value className="text-notch-danger">{Math.round(quota.extraUsagePercent)}%</Value>}
          />
        )}
      </List>
    </div>
  );
}

function percent(w: QuotaWindow): number {
  return Math.min(100, Math.max(0, w.percent ?? 0));
}

/** Quiet under 70%, amber approaching the cap, red near it — beside the number, never instead of it. */
function tone(w: QuotaWindow): Tone {
  const pct = percent(w);
  return pct >= 90 ? "danger" : pct >= 70 ? "warn" : "ink";
}
