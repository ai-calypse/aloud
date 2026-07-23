import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  agentStage,
  agentHue,
  hash,
  stageCat,
  stageSeed,
  syncStages,
  STAGE_LABEL,
} from "./agent-stage";
import type { AgentSession } from "./bindings";

const base: AgentSession = {
  id: "s1",
  agent: "claude",
  title: "aloud",
  prompt: null,
  cwd: null,
  tool: null,
  model: null,
  branch: null,
  lastActivity: null,
  subagents: [],
  busy: false,
  transient: null,
};

describe("agentStage", () => {
  it("a pending permission outranks everything", () => {
    expect(agentStage({ ...base, busy: true, tool: "Bash: cargo test" }, true)).toBe(
      "waiting_input",
    );
  });

  it("classifies the running tool by its flattened name", () => {
    expect(agentStage({ ...base, busy: true, tool: "Bash: cargo test" }, false)).toBe("running");
    expect(agentStage({ ...base, busy: true, tool: "Edit: src/x.ts" }, false)).toBe("editing");
    expect(agentStage({ ...base, busy: true, tool: "Read: src/x.ts" }, false)).toBe("reading");
    expect(agentStage({ ...base, busy: true, tool: "Grep: TODO" }, false)).toBe("reading");
    expect(agentStage({ ...base, busy: true, tool: "TodoWrite" }, false)).toBe("planning");
    expect(agentStage({ ...base, busy: true, tool: "ExitPlanMode" }, false)).toBe("planning");
  });

  it("busy with no tool is thinking; not busy is idle", () => {
    expect(agentStage({ ...base, busy: true, tool: null }, false)).toBe("thinking");
    expect(agentStage({ ...base, busy: false, tool: null }, false)).toBe("idle");
  });

  it("a transient state from Rust outranks everything, including asking", () => {
    // Denied wins even mid-tool with a permission pending.
    expect(
      agentStage({ ...base, busy: true, tool: "Bash: rm -rf", transient: "denied" }, true),
    ).toBe("denied");
    expect(agentStage({ ...base, transient: "session_ended" }, false)).toBe("session_ended");
  });

  it("an unknown transient value is ignored, not shown", () => {
    expect(agentStage({ ...base, busy: false, transient: "bogus" }, false)).toBe("idle");
  });
});

describe("stageCat", () => {
  it("stays within the folder's image count and is stable per seed", () => {
    const a = stageCat("thinking", "s1");
    expect(a).toBe(stageCat("thinking", "s1")); // stable
    expect(a).toMatch(/^\/cats\/thinking\/[1-5]\.png$/); // in range (5 images)
  });

  it("editing has 3 images — never indexes past them", () => {
    for (const seed of ["a", "b", "c", "zzz", "session-123"]) {
      expect(stageCat("editing", seed)).toMatch(/^\/cats\/editing\/[1-3]\.png$/);
    }
  });
});

describe("hash / agentHue", () => {
  it("hash is non-negative and stable per string", () => {
    expect(hash("aloud")).toBe(hash("aloud"));
    expect(hash("")).toBeGreaterThanOrEqual(0);
    for (const s of ["", "a", "session-1", "pid:4022", "🐱"]) {
      expect(hash(s)).toBeGreaterThanOrEqual(0);
    }
  });

  it("agentHue stays in the red-skipping 40–320 band and is stable per id", () => {
    for (const id of ["preview:0", "preview:1", "sess-abc", "pid:99"]) {
      const h = agentHue(id);
      expect(h).toBeGreaterThanOrEqual(40);
      expect(h).toBeLessThanOrEqual(320);
      expect(agentHue(id)).toBe(h); // stable
    }
  });

  it("spreads near-identical ids far apart, not a degree away", () => {
    // The ×137 spread is why preview:0/1/2 do not all land on the same hue —
    // a plain modulo clustered them within a couple of degrees.
    const hues = ["preview:0", "preview:1", "preview:2"].map(agentHue);
    const maxGap = Math.max(...hues) - Math.min(...hues);
    expect(maxGap).toBeGreaterThan(20);
  });
});

describe("stageSeed / syncStages — a fresh cat on each stage re-entry, stable within", () => {
  it("returns the same seed while the agent stays in one stage", () => {
    syncStages([{ id: "sess-A", stage: "editing" }]);
    const first = stageSeed("sess-A", "editing");
    syncStages([{ id: "sess-A", stage: "editing" }]); // another poll, same stage
    expect(stageSeed("sess-A", "editing")).toBe(first); // still stable
  });

  it("changes the seed when the agent leaves and re-enters a stage", () => {
    syncStages([{ id: "sess-B", stage: "editing" }]);
    const editing1 = stageSeed("sess-B", "editing");
    syncStages([{ id: "sess-B", stage: "running" }]); // passes through another stage
    syncStages([{ id: "sess-B", stage: "editing" }]); // back to editing
    const editing2 = stageSeed("sess-B", "editing");
    expect(editing2).not.toBe(editing1); // re-entry → different seed → likely different cat
  });

  it("keeps different sessions independent", () => {
    syncStages([
      { id: "sess-C", stage: "reading" },
      { id: "sess-D", stage: "reading" },
    ]);
    expect(stageSeed("sess-C", "reading")).not.toBe(stageSeed("sess-D", "reading"));
  });

  it("prunes sessions that are gone, so the map does not grow unbounded", () => {
    syncStages([{ id: "gone", stage: "editing" }]);
    const before = stageSeed("gone", "editing");
    // A later poll no longer lists it — its entry is dropped, so a session that
    // reuses the id starts fresh at counter 1 rather than inheriting the old one.
    syncStages([{ id: "other", stage: "idle" }]);
    syncStages([{ id: "gone", stage: "editing" }]);
    expect(stageSeed("gone", "editing")).toBe(before); // reset to :1, same as first time
  });
});

// Guards the `ponytail:` hardcoded counts in agent-stage.ts against the art on
// disk: add or remove a cat and forget the constant, and this fails rather than
// the notch silently mispicking (or 404-ing a missing file).
describe("stage art stays in sync with public/cats", () => {
  const catsDir = fileURLToPath(new URL("../../public/cats", import.meta.url));

  for (const stage of Object.keys(STAGE_LABEL) as (keyof typeof STAGE_LABEL)[]) {
    it(`${stage}: every image stageCat can pick exists on disk`, () => {
      const onDisk = readdirSync(`${catsDir}/${stage}`).filter((f) => f.endsWith(".png"));
      // Probe enough seeds to hit every index the hash can produce.
      const referenced = new Set(
        Array.from({ length: 200 }, (_, i) => stageCat(stage, `seed-${i}`)),
      );
      for (const url of referenced) {
        expect(existsSync(`${catsDir}${url.replace("/cats", "")}`)).toBe(true);
      }
      // And no shipped image is unreachable — the count is not short.
      expect(referenced.size).toBe(onDisk.length);
    });
  }
});
