import { describe, expect, it } from "vitest";
import type { Entry } from "@/lib/bindings";
import type { Run } from "@/lib/agent-steps";
import { justFinished, liveRun, voiceRow, voiceStats } from "./voice-runs";

// Local 3pm, so "today" does not depend on the machine's time zone.
const NOW = new Date(2026, 9, 5, 15, 0, 0).getTime();
const at = (secondsAgo: number) => NOW / 1000 - secondsAgo;

function entry(over: Partial<Entry>): Entry {
  return {
    id: 1,
    createdAt: at(30),
    kind: "command",
    rawTranscript: "pause the music",
    finalTranscript: "Paused Spotify.",
    success: true,
    appName: null,
    durationMs: 900,
    utteranceId: 1,
    audioPath: null,
    ...over,
  };
}

describe("a voice row's words, per kind", () => {
  it("says a clean dictation was pasted, and what was pasted only when cleanup changed it", () => {
    const same = voiceRow(
      entry({ kind: "dictation", rawTranscript: "see you then", finalTranscript: "see you then" }),
      NOW,
    );
    expect(same.result).toBe("Pasted");
    const cleaned = voiceRow(
      entry({ kind: "dictation", rawTranscript: "um see you then", finalTranscript: "See you then." }),
      NOW,
    );
    expect(cleaned.said).toBe("um see you then");
    expect(cleaned.result).toBe("Pasted · See you then.");
  });

  it("gives a command's answer as its one line, whole when expanded", () => {
    const long = "Canberra. ".repeat(40).trim();
    const row = voiceRow(entry({ finalTranscript: long }), NOW);
    expect(row.result.endsWith("…")).toBe(true);
    expect(row.result.length).toBeLessThan(long.length);
    expect(row.full).toBe(long);
  });

  it("names a failure in words, so it never reads as colour alone", () => {
    const row = voiceRow(entry({ success: false, finalTranscript: "Gmail is not connected" }), NOW);
    expect(row.failed).toBe(true);
    expect(row.result).toBe("Failed · Gmail is not connected");
  });

  it("does not leave a blank line for a run with nothing to say", () => {
    expect(voiceRow(entry({ finalTranscript: "" }), NOW).result).toBe("Done");
    expect(voiceRow(entry({ success: false, finalTranscript: "" }), NOW).result).toBe(
      "Failed · no reason given",
    );
  });

  it("dates the row from now to days", () => {
    const when = (s: number) => voiceRow(entry({ createdAt: at(s) }), NOW).when;
    expect(when(20)).toBe("now");
    expect(when(4 * 60)).toBe("4m");
    expect(when(3 * 3600)).toBe("3h");
    expect(when(50 * 3600)).toBe("2d");
  });
});

const run = (over: Partial<Run>): Run => ({ heard: "pause the music", steps: [], ...over });

describe("the live run and the list", () => {
  it("draws a run that is still going, whatever the list holds", () => {
    expect(liveRun(run({}), [entry({})])).not.toBeNull();
  });

  it("hands a finished run over to its row, so the answer is not on screen twice", () => {
    const done = run({ finished: "Paused Spotify." });
    expect(liveRun(done, [entry({})])).toBeNull();
  });

  it("keeps drawing a finished run whose row is not there, rather than losing the answer", () => {
    const done = run({ finished: "Paused Spotify." });
    expect(liveRun(done, [])).toBe(done);
    expect(liveRun(run({ error: "boom" }), [entry({ rawTranscript: "something else" })])).not.toBeNull();
  });

  it("opens the row of the run that just ended, and nothing for one still going", () => {
    const done = run({ finished: "ok" });
    expect(justFinished(done, [entry({ id: 9 })])).toBe(9);
    expect(justFinished(run({}), [entry({ id: 9 })])).toBeNull();
    expect(justFinished(null, [entry({ id: 9 })])).toBeNull();
  });
});

describe("voiceStats", () => {
  it("counts today from local midnight and failures among commands", () => {
    const rows = [
      entry({ id: 1, createdAt: at(60) }),
      entry({ id: 2, createdAt: at(60), success: false }),
      entry({ id: 3, createdAt: at(10 * 86_400) }),
    ];
    const stats = voiceStats(rows, NOW);
    expect(stats.failed).toBe(1);
    expect(stats.today).toBe(2);
  });
});
