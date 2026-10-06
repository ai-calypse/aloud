import { describe, expect, it, vi } from "vitest";
import { commands } from "@/lib/bindings";
import { ACTIONS, mediaTarget, runAction, type NotchAction } from "./actions";
import { SNAPS } from "./snaps";

vi.mock("@/lib/bindings", () => ({
  commands: {
    mediaControl: vi.fn(async () => ({ status: "ok", data: null })),
    mediaNowPlaying: vi.fn(async () => ({ now: { player: "spotify" } })),
  },
}));

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
    for (const id of ["agents.open", "result.show", "files.undoTidy", "media.toggle", "focus.pause", "focus.resume", "focus.awakeOff", "calendar.join"]) expect(ids).toContain(id);
  });

  // aloud-mac names these in its islands' Show buttons, and the view's
  // buttons and ⌘K must agree on which positions exist.
  it("has the Mac actions Rust sends and one snap per position", () => {
    const ids = ACTIONS.map((a) => a.id);
    for (const id of ["mac.power", "mac.privacy"]) expect(ids).toContain(id);
    for (const snap of SNAPS) expect(ids).toContain(`mac.snap.${snap.id}`);
  });
});

describe("media island buttons", () => {
  // The failure: every media island pausing whichever source the Media view
  // was showing. Rust puts the source in the button's args; this must use it.
  it("drives the browser tab the island names, not the chosen one", async () => {
    const tab = { player: "Brave Browser", tab: "2/3", host: "youtube.com" };
    await runAction("media.toggle", ctx, tab);
    expect(commands.mediaControl).toHaveBeenLastCalledWith("browser", "toggle", {
      app: "Brave Browser",
      bundleId: null,
      host: "youtube.com",
      tab: "2/3",
    });
  });

  it("names no tab for a player, the any-app source or ⌘K", async () => {
    expect(mediaTarget({ player: "Spotify" })).toBeNull();
    await runAction("media.toggle", ctx, { player: "Spotify" });
    expect(commands.mediaControl).toHaveBeenLastCalledWith("spotify", "toggle", null);
    await runAction("media.toggle", ctx, { player: "mediaRemote" });
    expect(commands.mediaControl).toHaveBeenLastCalledWith("mediaRemote", "toggle", null);
    await runAction("media.toggle", ctx, {});
    expect(commands.mediaControl).toHaveBeenLastCalledWith("spotify", "toggle", null);
  });
});
