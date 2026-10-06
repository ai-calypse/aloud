import { describe, expect, it } from "vitest";
import { homeNav, shellState } from "./hover";

const resting = { browsing: false, resting: true, hovering: true, dwelled: true, whisperOpen: false };

describe("the hover-opened shell", () => {
  it("waits for the pointer to rest before opening, so a pass to the menu bar does nothing", () => {
    expect(shellState({ ...resting, dwelled: false })).toBe("closed");
    expect(shellState(resting)).toBe("peek");
  });

  it("closes when the pointer leaves, because nothing was clicked", () => {
    expect(shellState({ ...resting, hovering: false })).toBe("closed");
  });

  it("stays once the user clicked or typed in it, however the pointer moves", () => {
    expect(shellState({ ...resting, browsing: true })).toBe("open");
    expect(shellState({ ...resting, browsing: true, hovering: false, dwelled: false })).toBe("open");
  });

  it("lets a whisper's own buttons have the hover, then opens Home once it is gone", () => {
    expect(shellState({ ...resting, whisperOpen: true })).toBe("closed");
    expect(shellState({ ...resting, whisperOpen: false })).toBe("peek");
  });

  it("never opens over a dictation, a run or a toast", () => {
    expect(shellState({ ...resting, resting: false })).toBe("closed");
  });
});

describe("homeNav", () => {
  it("lands on Home and leaves every other remembered subsection alone", () => {
    const left = { section: "media" as const, subs: { media: "lyrics", agents: "coding", now: "activity" } };
    expect(homeNav(left)).toEqual({
      section: "now",
      subs: { media: "lyrics", agents: "coding", now: "overview" },
    });
    // The user's own place is not rewritten by the glance.
    expect(left.section).toBe("media");
  });
});
