import { createContext, useContext, type Dispatch, type RefObject, type SetStateAction } from "react";
import type {
  AgentPermission,
  AgentSession,
  ApprovalReply,
  Island,
  IslandAction,
} from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import type { PeekFn } from "@/features/overlay/agentIdentity";
import type { Tab } from "@/features/overlay/TabStrip";
import type { SectionId } from "./sections";

/**
 * What the notch knows right now. The overlay owns every piece of it (the
 * event listeners live there); the registry's views read it from here rather
 * than through props, so a view is a plain component the registry can name.
 */
export type NotchData = {
  /** The voice command running, or the last one still on screen. */
  agent: Run | null;
  /** Coding agents (Claude Code, Codex), busy-first from Rust. */
  sessions: AgentSession[];
  /** Approvals waiting on the user, newest first. */
  permissions: AgentPermission[];
  /** The island queue, ranked by Rust. */
  islands: Island[];
};

export type NotchContextValue = NotchData & {
  setAgent: Dispatch<SetStateAction<Run | null>>;
  /** Runs an island's button through the action registry; true when it did. */
  act: (action: IslandAction) => Promise<boolean>;
  /** The coding agent drilled into, by session id. */
  selected: string | null;
  select: (id: string | null) => void;
  resolvePermission: (id: string, reply: ApprovalReply) => void;
  showPeek: PeekFn;
  /** The voice run's tab strip: the run itself, plus coding-agent composers. */
  voice: {
    tabs: Tab[];
    active: number;
    select: (id: number) => void;
    add: () => void;
    close: (id: number) => void;
    drop: (id: number) => void;
  };
  /** Followed to the newest step while a run streams. */
  bodyRef: RefObject<HTMLDivElement | null>;
  navigate: (section: SectionId, sub?: string) => void;
  /** Closes the whole panel, the way Escape does. */
  close: () => void;
  /** ⌘K is open in the body. The overlay owns it so closing the panel resets it. */
  palette: boolean;
  setPalette: (open: boolean) => void;
};

const NotchContext = createContext<NotchContextValue | null>(null);

export const NotchProvider = NotchContext.Provider;

export function useNotch(): NotchContextValue {
  const value = useContext(NotchContext);
  if (!value) throw new Error("useNotch outside the notch panel");
  return value;
}
