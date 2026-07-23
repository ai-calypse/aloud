import type { AgentSession } from "./bindings";

/**
 * The agent's lifecycle stage, derived from the little Aloud actually observes:
 * a `busy` bool and the one running tool name. Only the stages that fall out of
 * *current* data live here — the rest (test pass/fail, denied, stuck,
 * rate-limited, session-ended) need signal that is captured but discarded today,
 * or not on the wire at all, so they are deliberately absent rather than faked.
 *
 * The cat for each stage lives at `public/cats/<stage>/<n>.png`, indexed by the
 * manifest in `public/cats/states.json`.
 */
export type AgentStage =
  | "idle"
  | "thinking"
  | "planning"
  | "reading"
  | "editing"
  | "running"
  | "waiting_input"
  | "denied"
  | "session_ended";

/** A friendly line for the hover-peek caption — names what the cat means. */
export const STAGE_LABEL: Record<AgentStage, string> = {
  idle: "waiting",
  thinking: "thinking",
  planning: "planning",
  reading: "reading files",
  editing: "editing files",
  running: "running command",
  waiting_input: "wants permission",
  denied: "denied",
  session_ended: "session ended",
};

/**
 * How many images each stage folder holds, so the picker can vary the cat
 * without fetching the manifest at runtime. Kept honest by `agent-stage.test.ts`,
 * which asserts each count against the folders on disk — so adding or removing
 * art that forgets this map fails the test rather than silently mispicking.
 * ponytail: hardcoded counts, not a manifest fetch — one JSON round-trip per
 * notch open is not worth it for a number that changes when *we* add art.
 */
const STAGE_IMAGE_COUNT: Record<AgentStage, number> = {
  idle: 4,
  thinking: 5,
  planning: 4,
  reading: 4,
  editing: 3,
  running: 4,
  waiting_input: 4,
  denied: 3,
  session_ended: 2,
};

/**
 * The tool classes that name a stage. The `tool` field arrives already
 * flattened to `"Bash: cargo test"`, so we match on the name before the colon.
 */
const PLANNING_TOOLS = new Set(["TodoWrite", "ExitPlanMode"]);
const READING_TOOLS = new Set(["Read", "Grep", "Glob", "LS", "WebFetch", "WebSearch"]);
const EDITING_TOOLS = new Set(["Edit", "Write", "NotebookEdit", "MultiEdit"]);
const RUNNING_TOOLS = new Set(["Bash", "BashOutput"]);

/** The tool name out of the flattened `"Bash: cargo test"` one-liner. */
function toolName(tool: string): string {
  const colon = tool.indexOf(":");
  return (colon === -1 ? tool : tool.slice(0, colon)).trim();
}

/**
 * The stage for a session. `asking` is whether a permission is pending for it —
 * that outranks everything, because a blocked agent is the one thing the user
 * has to act on. Then the running tool names the stage; a busy agent with no
 * tool is between calls ("thinking"); anything else is idle.
 */
export function agentStage(session: AgentSession, asking: boolean): AgentStage {
  // A one-shot state Rust just reported (denied, ended) outranks everything —
  // it is the freshest thing that happened and decays on its own. Its string is
  // the stage name verbatim (Rust's `Transient::as_str` is the only producer);
  // guarding against an unknown value keeps a wire change from picking a cat
  // from a folder that does not exist.
  if (session.transient && session.transient in STAGE_IMAGE_COUNT) {
    return session.transient as AgentStage;
  }
  if (asking) return "waiting_input";
  if (session.tool) {
    const name = toolName(session.tool);
    // A weak signal — laying out TODOs or exiting plan mode is the closest the
    // agent gets to "planning" in the current data. Not truly distinct from
    // thinking, but the two tools only fire when it is.
    if (PLANNING_TOOLS.has(name)) return "planning";
    if (RUNNING_TOOLS.has(name)) return "running";
    if (EDITING_TOOLS.has(name)) return "editing";
    if (READING_TOOLS.has(name)) return "reading";
    // A tool we do not classify still means work is happening — closer to
    // "reading" (inspecting something) than idle.
    return "reading";
  }
  if (session.busy) return "thinking";
  return "idle";
}

/**
 * A cat for the stage, chosen by hashing `seed`. Pure: same seed → same cat.
 * The variation over an agent's life comes from what seed the caller passes —
 * see [`stageSeed`].
 */
export function stageCat(stage: AgentStage, seed = ""): string {
  const count = STAGE_IMAGE_COUNT[stage];
  let hash = 0;
  for (let i = 0; i < seed.length; i++) hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  const n = (Math.abs(hash) % count) + 1;
  return `/cats/${stage}/${n}.png`;
}

/**
 * Per session: the stage it is currently in, and a counter bumped every time it
 * enters a new stage. Module-level so it survives re-renders without React
 * state — reading it must not itself trigger a render. Mutated only by
 * [`syncStages`]; [`stageSeed`] is a pure read.
 */
const entries = new Map<string, { stage: AgentStage; enter: number }>();

/**
 * Reconcile the counter map with the live sessions, once per render, before any
 * seed is read. Two jobs: bump a session's counter when it enters a new stage
 * (so the cat differs from the last time it was in that stage), and drop
 * entries for sessions that no longer exist (else the map grows forever as
 * sessions churn). Kept out of render — mutating during render is impure and
 * would fight StrictMode/concurrent rendering.
 */
export function syncStages(sessions: { id: string; stage: AgentStage }[]): void {
  const live = new Set<string>();
  for (const { id, stage } of sessions) {
    live.add(id);
    const seen = entries.get(id);
    if (!seen || seen.stage !== stage) {
      entries.set(id, { stage, enter: (seen?.enter ?? 0) + 1 });
    }
  }
  for (const id of entries.keys()) {
    if (!live.has(id)) entries.delete(id);
  }
}

/**
 * The seed for a session's current stage — pure. Fixed *while* the agent stays
 * in the stage (so the cat does not flicker as the 3s poll re-renders it) and
 * changed on re-entry, because [`syncStages`] bumped the counter. Falls back to
 * a bare seed for a session `syncStages` has not seen yet.
 *
 * Read once per render for a session and pass the result to both the avatar and
 * its hover-peek, so they always show the same cat.
 */
export function stageSeed(sessionId: string, stage: AgentStage): string {
  const enter = entries.get(sessionId)?.enter ?? 1;
  return `${sessionId}:${stage}:${enter}`;
}
