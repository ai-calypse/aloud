// The engine's only interface: newline-delimited JSON-RPC over stdio, so the Swift shell (or any
// process) can drive it. Requests: {"id", "method", "params"}; replies: {"id", "result"} or
// {"id", "error"}. Running tasks also push {"event": "task", ...} lines until they finish.
import { appendFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { UserError } from "./lib/jev.ts";
import * as web from "./web.ts";

const METHODS: Record<string, (p: any) => Promise<unknown>> = {
  "web.start": web.start,
  "task.progress": web.progress,
  "task.approve": web.approve,
  "task.stop": web.stop,
  ping: async () => ({ ok: true }),
};

const send = (msg: unknown) => process.stdout.write(JSON.stringify(msg) + "\n");

// A local record of every request and how it ended (no audio, no screenshots), so a test can be reviewed later.
const LOG = join(process.env.VOICE_MAC_DATA ?? join(import.meta.dirname, "data"), "log.jsonl");
const log = (entry: Record<string, unknown>) =>
  mkdir(join(LOG, ".."), { recursive: true }).then(() => appendFile(LOG, JSON.stringify({ at: new Date().toISOString(), ...entry }) + "\n")).catch(() => {});

/** Push a task's new steps and status until it ends, so the shell never has to poll. */
async function watch(id: string) {
  let from = 0;
  for (;;) {
    const p: any = await web.progress({ id, from }).catch(() => null);
    if (!p) return;
    if (p.steps.length || ["done", "stuck", "stopped", "error", "waiting"].includes(p.status)) send({ event: "task", id, ...p, live: undefined, steps: p.steps.map(({ shot, ...s }: any) => s) }); // screenshots stay in task.progress
    from += p.steps.length;
    if (["done", "stuck", "stopped", "error"].includes(p.status)) {
      const all: any = await web.progress({ id, from: 0 }).catch(() => null);
      log({ kind: "task", id, goal: all?.goal, app: all?.app, status: all?.status, error: all?.error, answer: all?.answer, elapsed_ms: all?.elapsed_ms,
        steps: all?.steps?.map((s: any) => ({ op: s.op, target: s.target?.label, text: s.text, note: s.note, url: s.url })) });
      return;
    }
    await new Promise((r) => setTimeout(r, 300));
  }
}

createInterface({ input: process.stdin }).on("line", async (line) => {
  let req: { id?: unknown; method?: string; params?: unknown };
  try {
    req = JSON.parse(line);
  } catch {
    return send({ id: null, error: { message: "Not JSON." } });
  }
  const fn = METHODS[String(req.method)];
  if (!fn) return send({ id: req.id, error: { message: `Unknown method ${req.method}.` } });
  try {
    const result: any = await fn(req.params ?? {});
    send({ id: req.id, result });
    if (req.method !== "task.progress") log({ kind: "call", method: req.method, params: req.params, result });
    const taskId = req.method === "web.start" ? result?.id : undefined;
    if (taskId) watch(taskId);
  } catch (e) {
    const message = String((e as Error).message ?? e).split("\n")[0];
    send({ id: req.id, error: { message, user: e instanceof UserError } });
    log({ kind: "error", method: req.method, message });
  }
});
// The app owns this process: when it quits (stdin closes), exit too, taking axd and the browser with it.
process.stdin.on("end", () => process.exit(0));
send({ event: "ready", methods: Object.keys(METHODS) });
