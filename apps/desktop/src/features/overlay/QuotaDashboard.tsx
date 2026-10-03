import { useEffect, useState } from "react";
import { commands, type Quota, type QuotaWindow } from "@/lib/bindings";
import { resetsIn } from "@/lib/format";

/**
 * The agent sessions, once the notch has been clicked open. Each row drills
 * The plan-quota dashboard: Claude's rolling usage limits as labeled bars.
 * Reads the Keychain token + usage API through `agent_quota`, polling on an
 * interval. Silent on error (not signed in / token expired) except for a quiet
 * one-line note — a broken quota read must not shout over the agent list.
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

  // Idle: a quiet button, no fetch, no prompt until asked.
  if (!on && !quota) {
    return (
      <div className="quota">
        <button className="quota__toggle" onClick={() => setOn(true)}>
          Show plan usage
        </button>
        {error && <p className="quota__note">{error}</p>}
      </div>
    );
  }

  const windows: [string, QuotaWindow | null][] = quota
    ? [
        ["5-hour", quota.fiveHour],
        ["Week", quota.sevenDay],
        ["Week · Opus", quota.sevenDayOpus],
      ]
    : [];
  const shown = windows.filter(([, w]) => w != null);

  return (
    <div className="quota">
      {shown.length > 0 ? (
        <>
          {shown.map(([label, w]) => (
            <QuotaBar key={label} label={label} window={w!} />
          ))}
          {quota?.extraUsagePercent != null && quota.extraUsagePercent > 0 && (
            <div className="quota__extra">
              Extra usage · {Math.round(quota.extraUsagePercent)}%
            </div>
          )}
        </>
      ) : (
        <p className="quota__note">{error ?? "No usage data"}</p>
      )}
    </div>
  );
}

/** One rolling limit: a label, a fill bar tinted by how full, % and reset. */
function QuotaBar({ label, window: w }: { label: string; window: QuotaWindow }) {
  const pct = Math.min(100, Math.max(0, w.percent ?? 0));
  // Green under half, amber approaching, red near the cap — a shape a glance reads.
  const level = pct >= 90 ? "high" : pct >= 70 ? "mid" : "low";
  return (
    <div className="quota__row">
      <div className="quota__head">
        <span className="quota__label">{label}</span>
        <span className="quota__figures">
          <span className="quota__pct" data-level={level}>
            {Math.round(pct)}%
          </span>
          {w.resetsAt && (
            <span className="quota__reset">· resets in {resetsIn(w.resetsAt)}</span>
          )}
        </span>
      </div>
      <div className="quota__track">
        <div className="quota__fill" data-level={level} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}
