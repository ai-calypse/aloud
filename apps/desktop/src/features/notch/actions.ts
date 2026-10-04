import { commands } from "@/lib/bindings";
import type { SectionId } from "./sections";

/** What an action may do to the notch itself. */
export type ActionContext = {
  /** Opens the panel, if it is closed, at a section and subsection. */
  open: (section: SectionId, sub?: string) => void;
};

/**
 * Something the notch can do, by name. One registry for every way in: the
 * ⌘K palette lists these, an island's buttons name them (Rust sends the id,
 * never code), and the voice agent can call them by id next.
 */
export type NotchAction = {
  id: string;
  title: string;
  /** Other words people would search for it by. */
  keywords?: string[];
  /** Where it lives, for the palette's hint and for grouping. */
  section?: SectionId;
  subsection?: string;
  /**
   * Arguments it cannot run without. An action that needs any is only for a
   * caller that has them — an island, the agent — and ⌘K does not offer it.
   */
  args?: string[];
  /**
   * Does it. Resolves true when it did what it says, which is when an island
   * that offered it may go; false keeps the island up to say so.
   */
  run: (ctx: ActionContext, args: Record<string, string>) => boolean | Promise<boolean>;
};

export const ACTIONS: NotchAction[] = [
  {
    id: "agents.open",
    title: "Open coding agents",
    keywords: ["claude", "codex", "approvals", "permission"],
    section: "agents",
    subsection: "coding",
    run: (ctx) => {
      ctx.open("agents", "coding");
      return true;
    },
  },
  {
    id: "agents.voice",
    title: "Show the last voice command",
    keywords: ["command", "run", "steps"],
    section: "agents",
    subsection: "voice",
    run: (ctx) => {
      ctx.open("agents", "voice");
      return true;
    },
  },
  {
    id: "agents.usage",
    title: "Show plan usage",
    keywords: ["quota", "limits", "claude"],
    section: "agents",
    subsection: "usage",
    run: (ctx) => {
      ctx.open("agents", "usage");
      return true;
    },
  },
  {
    // What a finished row's Show does: bring back what a write made.
    id: "result.show",
    title: "Show what was made",
    args: ["app", "target"],
    run: (_, { app, target }) => {
      void commands.showResult(app, target);
      return true;
    },
  },
  {
    // A tidy is the one write with a reverse already built.
    id: "files.undoTidy",
    title: "Undo a tidy",
    args: ["folder"],
    run: async (_, { folder }) => (await commands.undoTidy(folder)).status === "ok",
  },
];

/**
 * Runs an action by id. False for an id nobody registered, for missing
 * arguments, and for a run that throws — the browser preview has no Tauri,
 * and an island button must fail visibly rather than take the overlay down.
 */
export async function runAction(
  id: string,
  ctx: ActionContext,
  args: Record<string, string> = {},
  registry: NotchAction[] = ACTIONS,
): Promise<boolean> {
  const action = registry.find((a) => a.id === id);
  if (!action) return false;
  if (action.args?.some((name) => !(name in args))) return false;
  try {
    return await action.run(ctx, args);
  } catch {
    return false;
  }
}
