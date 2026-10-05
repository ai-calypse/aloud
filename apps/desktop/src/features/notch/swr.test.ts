import { describe, expect, it } from "vitest";
import { revalidate } from "./swr";

type R = { status: "ok"; data: string[] } | { status: "error"; error: string };
const later = <T>(value: T, ms: number) => new Promise<T>((done) => setTimeout(() => done(value), ms));

function run(peek: () => Promise<string[] | null>, fresh: () => Promise<R>) {
  const seen: string[] = [];
  const errors: string[] = [];
  const done = revalidate({
    peek,
    fresh,
    onItems: (items) => seen.push(items.join(",")),
    onError: (e) => errors.push(e),
  });
  return { seen, errors, done };
}

describe("revalidate", () => {
  // The user's complaint: waiting on a slow read to see rows that were known.
  it("shows the last known rows before the real read lands, then the real ones", async () => {
    const { seen, done } = run(
      () => Promise.resolve(["old"]),
      () => later<R>({ status: "ok", data: ["new"] }, 20),
    );
    await Promise.resolve();
    await Promise.resolve();
    expect(seen).toEqual(["old"]);
    await done;
    expect(seen).toEqual(["old", "new"]);
  });

  it("does not paint stale rows over a read that already landed", async () => {
    const { seen, done } = run(
      () => later(["old"], 20),
      () => Promise.resolve<R>({ status: "ok", data: ["new"] }),
    );
    await done;
    expect(seen).toEqual(["new"]);
  });

  it("keeps the cached rows and reports the error when the read fails", async () => {
    const { seen, errors, done } = run(
      () => Promise.resolve(["old"]),
      () => later<R>({ status: "error", error: "not allowed" }, 5),
    );
    await done;
    expect(seen).toEqual(["old"]);
    expect(errors).toEqual(["not allowed"]);
  });

  it("is just the read when there is nothing cached, or the cache cannot be reached", async () => {
    const none = run(
      () => Promise.resolve(null),
      () => Promise.resolve<R>({ status: "ok", data: ["new"] }),
    );
    await none.done;
    expect(none.seen).toEqual(["new"]);
    const broken = run(
      () => Promise.reject(new Error("no ipc")),
      () => Promise.resolve<R>({ status: "ok", data: ["new"] }),
    );
    await broken.done;
    expect(broken.seen).toEqual(["new"]);
  });
});
