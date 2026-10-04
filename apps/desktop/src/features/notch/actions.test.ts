import { describe, expect, it, vi } from "vitest";
import { ACTIONS, runAction, type NotchAction } from "./actions";

const ctx = { open: vi.fn() };

describe("runAction", () => {
  const registry: NotchAction[] = [
    { id: "ok", title: "Ok", run: () => true },
    { id: "needs", title: "Needs", args: ["folder"], run: (_, a) => a.folder === "Downloads" },
    {
      id: "throws",
      title: "Throws",
      run: () => {
        throw new Error("no Tauri");
      },
    },
  ];

  it("runs a registered action with its arguments", async () => {
    expect(await runAction("ok", ctx, {}, registry)).toBe(true);
    expect(await runAction("needs", ctx, { folder: "Downloads" }, registry)).toBe(true);
  });

  // An island button naming something this build does not have, or sent
  // without what it needs, must fail visibly — never throw into the overlay.
  it("fails, without throwing, for an unknown id, a missing argument or a throwing run", async () => {
    expect(await runAction("nope", ctx, {}, registry)).toBe(false);
    expect(await runAction("needs", ctx, {}, registry)).toBe(false);
    expect(await runAction("throws", ctx, {}, registry)).toBe(false);
  });

  it("opens the panel where an action lives", async () => {
    await runAction("agents.open", ctx);
    expect(ctx.open).toHaveBeenCalledWith("agents", "coding");
  });
});

describe("ACTIONS", () => {
  // Rust names these in island buttons; a typo on either side is silent.
  it("has every action Rust sends, with unique ids", () => {
    const ids = ACTIONS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ["agents.open", "result.show", "files.undoTidy", "focus.pause", "focus.resume", "focus.awakeOff", "calendar.join"]) expect(ids).toContain(id);
  });
});
