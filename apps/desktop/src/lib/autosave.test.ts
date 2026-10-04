import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAutosaver } from "./autosave";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("createAutosaver", () => {
  it("saves once, with the latest text, after typing pauses", async () => {
    const save = vi.fn(async () => {});
    const saver = createAutosaver(save, 600);
    saver.schedule("h");
    saver.schedule("he");
    await vi.advanceTimersByTimeAsync(599);
    expect(save).not.toHaveBeenCalled();
    saver.schedule("hello");
    await vi.advanceTimersByTimeAsync(600);
    expect(save.mock.calls).toEqual([["hello"]]);
  });

  // The failure: the first save creates the file and returns its path; a
  // second one started before that returned would create another file.
  it("never runs two saves at once, and the newest text wins", async () => {
    const order: string[] = [];
    let release!: () => void;
    const save = vi.fn(async (text: string) => {
      order.push(`start ${text}`);
      await new Promise<void>((r) => (release = r));
      order.push(`end ${text}`);
    });
    const saver = createAutosaver(save, 10);
    saver.schedule("a");
    await vi.advanceTimersByTimeAsync(10);
    saver.schedule("ab");
    saver.schedule("abc");
    await vi.advanceTimersByTimeAsync(10);
    expect(order).toEqual(["start a"]);
    release();
    await vi.advanceTimersByTimeAsync(0);
    release();
    await vi.advanceTimersByTimeAsync(0);
    expect(order).toEqual(["start a", "end a", "start abc", "end abc"]);
  });

  it("flush saves immediately and resolves when everything is on disk", async () => {
    const save = vi.fn(async () => {});
    const saver = createAutosaver(save, 600);
    saver.schedule("note");
    await saver.flush();
    expect(save).toHaveBeenCalledWith("note");
    await vi.advanceTimersByTimeAsync(1000);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("keeps going after a save fails", async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error("disk")).mockResolvedValue(undefined);
    const saver = createAutosaver(save, 10);
    saver.schedule("a");
    await vi.advanceTimersByTimeAsync(10);
    saver.schedule("ab");
    await vi.advanceTimersByTimeAsync(10);
    expect(save).toHaveBeenLastCalledWith("ab");
  });
});
