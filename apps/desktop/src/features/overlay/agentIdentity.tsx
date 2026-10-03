import React from "react";
import type { AgentSession } from "@/lib/bindings";
import { stageCat, type AgentStage } from "@/lib/agent-stage";
import { timeAgo } from "@/lib/format";
import { useCatSettings } from "@/lib/cat-settings";
import { PetMascot } from "./PetMascot";

/**
 * An agent's avatar image, or a plain hue-tinted disc when the Cats setting is
 * off for agents. Every agent-cat site routes through here so the toggle has
 * one place to bite; `className` and `hue` let each caller keep its own size
 * and identity colour. `hue` comes from the session id (see `agentHue`), so the
 * fallback is still learnable identity even without the cat.
 */
export function AgentAvatar({
  stage,
  seed,
  className,
  ...rest
}: {
  stage: AgentStage;
  seed: string;
  /** Kept for call-site symmetry with the ring; the mascot ignores it. */
  hue?: number;
  className: string;
} & React.HTMLAttributes<HTMLElement>) {
  const cats = useCatSettings();
  if (!cats.agents) {
    // Cats off → the Claude Code banner mascot walks in the avatar box instead.
    return <PetMascot className={`${className} agent-avatar--off`} {...rest} />;
  }
  return (
    <img className={className} src={stageCat(stage, seed)} alt="" aria-hidden {...rest} />
  );
}

/** The agent's display name — what `agent-notch` puts in bold at the row head. */
export const AGENT_NAME: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
};

/** The row's dot state: busy mid-turn, live between tools, else done. */
export function rowState(session: AgentSession): "busy" | "live" | "done" {
  return session.busy ? "busy" : session.tool ? "live" : "done";
}

/**
 * The row's name: what the session is working on, not what it is —
 * `aloud-tauri @ feat/agent-notch`. The agent's own name is the fallback for
 * a session with no known folder, and its model tag says the rest.
 */
export function rowName(session: AgentSession): string {
  const project = session.cwd?.split("/").filter(Boolean).pop();
  if (!project) return AGENT_NAME[session.agent] ?? session.agent;
  return session.branch ? `${project} @ ${session.branch}` : project;
}

/** The `model · time` tag next to a row. */
export function rowTag(session: AgentSession): string {
  return [session.model ?? session.agent, session.lastActivity && timeAgo(session.lastActivity)]
    .filter(Boolean)
    .join(" · ");
}

/**
 * Show/hide the enlarged cat preview, anchored to the given avatar element.
 * `null` stage hides it.
 */
export type PeekFn = (stage: AgentStage | null, seed: string, anchor: HTMLElement | null) => void;
