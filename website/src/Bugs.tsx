import { useState, type FormEvent } from "react";
import apolocheese from "./assets/memes/apolocheese.webp";
import { GlossyButton, MacWindow } from "./components/ds";
import Sticker from "./components/Sticker";
import { track } from "./analytics";
import { useCopy } from "./copy";
import { useMeta } from "./seo";

type State = "idle" | "sending" | "done" | "error" | "unconfigured";

// Posts {"description", "email", "macos"} as JSON to VITE_BUGS_URL (any form backend:
// Formspree, a Cloudflare Worker, a Google Apps Script). Until it is set the page says
// so, so no report is silently dropped.
const ENDPOINT = import.meta.env.VITE_BUGS_URL as string | undefined;

const field = "rounded-[12px] border border-line bg-surface-page px-4 py-3 text-[16px] placeholder:text-ink-muted";

export default function Bugs() {
  const all = useCopy();
  const c = all.bugs;
  const message: Record<State, string> = { idle: "", sending: c.sending, done: c.done, error: c.error, unconfigured: c.unconfigured };
  const [state, setState] = useState<State>("idle");
  useMeta(c.title);

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!ENDPOINT) return setState("unconfigured");
    const f = new FormData(e.currentTarget);
    setState("sending");
    try {
      const res = await fetch(ENDPOINT, {
        method: "POST",
        headers: { "Content-Type": "application/json", Accept: "application/json" },
        body: JSON.stringify({ description: f.get("description"), email: f.get("email"), macos: f.get("macos") }),
      });
      setState(res.ok ? "done" : "error");
      if (res.ok) track("bug_report");
    } catch {
      setState("error");
    }
  }

  return (
    <main
      className="relative flex min-h-svh flex-col items-center justify-center gap-10 bg-blue-700 px-5 py-20 text-neutral-0 md:flex-row md:gap-16"
      style={{ backgroundImage: "radial-gradient(circle at center, #0b57d6 1.6px, transparent 2px)", backgroundSize: "58px 58px" }}
    >
      <div className="absolute left-5 top-5">
        <GlossyButton size="sm" tone="white" href="/">
          {all.common.backHome}
        </GlossyButton>
      </div>

      <div className="flex flex-col items-center gap-5 text-center md:items-start md:text-left">
        <Sticker rotate={-5} className="w-[190px] md:w-[280px]">
          <img src={apolocheese} alt="A grey and white cat with a slice of cheese under its chin, captioned: my apolocheese" />
        </Sticker>
        <h1 className="t-section">{c.h1}</h1>
        <p className="t-lede max-w-[420px]">{c.p}</p>
      </div>

      <MacWindow title={c.window} className="w-full max-w-[460px]">
        <form data-clarity-mask="True" onSubmit={submit} className="flex flex-col gap-4 p-6 text-ink">
          <label className="flex flex-col gap-2 text-[14px] font-medium">
            {c.what}
            <textarea name="description" required rows={5} placeholder={c.whatPlaceholder} className={`${field} resize-y font-normal`} />
          </label>
          <label className="flex flex-col gap-2 text-[14px] font-medium">
            {c.email}
            <input name="email" type="email" autoComplete="email" placeholder="you@email.com" className={`${field} font-normal`} />
          </label>
          <label className="flex flex-col gap-2 text-[14px] font-medium">
            {c.macos}
            <input name="macos" placeholder="e.g. 14.2" className={`${field} font-normal`} />
          </label>
          <div>
            <GlossyButton submit size="sm" disabled={state === "sending" || state === "done"}>
              {c.send}
            </GlossyButton>
          </div>
          <p role="status" key={state} className="pop min-h-5 text-[14px] text-ink-muted">
            {message[state]}
          </p>
        </form>
      </MacWindow>
    </main>
  );
}
