// The engine's only interface: newline-delimited JSON-RPC over stdio, so the Swift shell (or any
// process) can drive it. Requests: {"id", "method", "params"}; replies: {"id", "result"} or
// {"id", "error"}. Running tasks also push {"event": "task", ...} lines until they finish.
import { appendFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { ask, UserError } from "./lib/jev.ts";
import * as browser from "./browser.ts";
import * as mac from "./mac.ts";

browser.useMac(mac);
// Mac-app ("m-") and web ("w-") task ids belong to mac.ts; the same task.* methods serve both.
const either = (b: (p: any) => Promise<unknown>, m: (p: any) => Promise<unknown>) => (p: any) => (/^[mw]-/.test(String(p?.id ?? "")) ? m(p) : b(p));

const WHERE = {
  mac: { what: "Something on this Mac or in a Mac app: open, switch or quit apps, music, volume, windows, reminders, notes, files and folders, Calculator, System Settings, typing into an app", examples: ["open Safari", "pause the music", "remind me to call Ana at 5", "in Calculator, 12 times 3", "open my Downloads folder"] },
  web: { what: "Something on a website in the browser: open a site, search it, watch or play a video, read a value off a page, use a web dashboard", examples: ["open YouTube and play lofi", "search claude-code on GitHub", "what's the population of Tokyo on Wikipedia", "open my Dodo payments dashboard"] },
  service: { what: "Something in the user's connected accounts that has its own integration: email, calendar, GitHub issues and pull requests, Slack messages, contacts", examples: ["summarise my unread email", "what's on my calendar tomorrow", "list my open pull requests", "message the team on Slack"] },
  answer: { what: "A question to answer or text to write, with no action on the Mac or the web", examples: ["what's 15% of 80", "write a haiku about rain"] },
};
const DIRECT = 0.6; // routing confidence needed to skip the agent's language model

/** The fast path: one Jev request decides whether a spoken command is a Mac or web task and starts it
 *  directly; anything else (connected services, answers, unsure) is handed back to the caller's agent. */
async function command({ text, front }: { text?: string; front?: string }) {
  const t = String(text ?? "").trim();
  if (!t) return { deferred: true, where: "none" };
  // What is on screen is the context a follow-up leans on: "what's 90 + 7" with Calculator in front is
  // a Mac task, not a question to answer in the panel.
  const app = await mac.frontApp(front);
  const a: any = await ask({ request: t, front_app: app || "none" }, { where: { type: "choice", instructions: "Where should `request` be carried out? `front_app` is the app on screen right now: a request that can be done in it (arithmetic while Calculator is in front, typing while an editor is) is carried out there, not answered.", criteria: WHERE } }, "route command");
  const where = a.where.choice as keyof typeof WHERE;
  if (a.where.confidence < DIRECT || where === "service" || where === "answer") return { deferred: true, where, confidence: a.where.confidence };
  // Web tasks run in the user's default browser through accessibility, like any Mac app.
  const task = await mac.start({ goal: t, web: where === "web", front: app });
  return { id: task.id, where, confidence: a.where.confidence };
}

const METHODS: Record<string, (p: any) => Promise<unknown>> = {
  command,
  "task.start": browser.start,
  "mac.start": mac.start,
  "task.progress": either(browser.progress, mac.progress),
  "task.approve": either(browser.approve, mac.approve),
  "task.stop": either(browser.stop, mac.stop),
  "task.history": browser.history,
  utterance: browser.voice, // {audio: base64 16 kHz WAV, live?, pending?, mine?}
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
    const p: any = await (/^[mw]-/.test(id) ? mac.progress : browser.progress)({ id, from }).catch(() => null);
    if (!p) return;
    if (p.steps.length || ["done", "stuck", "stopped", "error", "waiting"].includes(p.status)) send({ event: "task", id, ...p, live: undefined, steps: p.steps.map(({ shot, ...s }: any) => s) }); // screenshots stay in task.progress
    from += p.steps.length;
    if (["done", "stuck", "stopped", "error"].includes(p.status)) {
      const all: any = await (/^[mw]-/.test(id) ? mac.progress : browser.progress)({ id, from: 0 }).catch(() => null);
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
    if (req.method === "utterance") {
      const { said, by, ms_whisper, intent, confidence, addressed, surface, did, task } = result;
      log({ kind: "utterance", said, by, ms_whisper, intent, confidence, addressed, surface, did, task: task?.id });
    } else if (req.method !== "task.progress") log({ kind: "call", method: req.method, params: req.params, result });
    const taskId = ["task.start", "mac.start", "command"].includes(String(req.method)) ? result?.id : result?.task?.id;
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
