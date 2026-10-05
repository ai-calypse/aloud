import { useState, type FormEvent } from "react";
import phoneCat from "./assets/memes/phone-cat.webp";
import animated from "./assets/windows-waitlist.webp";
import still from "./assets/windows-waitlist-still.webp";
import { GlossyButton, MacWindow } from "./components/ds";
import Sticker from "./components/Sticker";
import { track } from "./analytics";
import { useCopy } from "./copy";
import { useMeta } from "./seo";

type State = "idle" | "sending" | "done" | "error" | "unconfigured";

// api/form.ts emails each signup through ZeptoMail. VITE_WAITLIST_URL points the form at
// another backend (it gets the same JSON). The function answers 503 until its token is set,
// and the page says so rather than dropping the signup silently.
const ENDPOINT = (import.meta.env.VITE_WAITLIST_URL as string | undefined) || "/api/form";

export default function Waitlist() {
  const c = useCopy();
  const message: Record<State, string> = { idle: "", sending: c.waitlist.sending, done: c.waitlist.done, error: c.waitlist.error, unconfigured: c.waitlist.unconfigured };
  const [state, setState] = useState<State>("idle");
  useMeta(c.waitlist.title);

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setState("sending");
    try {
      const res = await fetch(ENDPOINT, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify({ kind: "waitlist", email: f.get("email"), platform: "windows", hp: f.get("hp") }),
      });
      setState(res.ok ? "done" : res.status === 503 ? "unconfigured" : "error");
      if (res.ok) track("waitlist_join");
    } catch {
      setState("error");
    }
  }

  return (
    <main className="relative isolate flex min-h-svh flex-col items-center overflow-hidden bg-blue-700 md:justify-center">
      {/* A blurred still of the same frame fills whatever the video does not cover,
          so showing the whole video never leaves hard bars. */}
      <img src={still} alt="" aria-hidden="true" className="absolute inset-0 -z-10 size-full scale-125 object-cover blur-2xl" />

      <div className="absolute left-5 top-5 z-20">
        <GlossyButton size="sm" tone="white" href="/">
          {c.common.backHome}
        </GlossyButton>
      </div>

      {/* The stage keeps the video's own shape, so the cat is never cropped: full width
          on phones (form underneath), as large as fits the screen on desktop (form over
          the empty bottom-left corner). */}
      <div className="relative mt-20 w-full md:mt-0 md:max-w-[calc(100svh*544/306)]">
        <picture>
          {/* reduced motion gets a still frame */}
          <source media="(prefers-reduced-motion: reduce)" srcSet={still} />
          <img src={animated} alt="A cracked windows screen with a cat poking its head through the glass" width={544} height={306} className="block w-full" />
        </picture>

        <div className="relative p-5 md:absolute md:bottom-[5%] md:left-[3%] md:w-[clamp(300px,34%,440px)] md:p-0">
          <Sticker rotate={6} className="absolute -top-[42px] right-9 z-10 w-[84px] md:-top-[70px] md:right-4 md:w-[104px]">
            <img src={phoneCat} alt="A crying cat holding a phone, waiting for windows" />
          </Sticker>
          <MacWindow title={c.waitlist.window}>
            <form data-clarity-mask="True" onSubmit={submit} className="flex flex-col gap-4 p-6">
              <h1 className="t-card">{c.waitlist.h1}</h1>
              <p className="text-ink-muted">{c.waitlist.p}</p>
              <input name="hp" tabIndex={-1} autoComplete="off" aria-hidden="true" className="absolute -left-[9999px] size-px opacity-0" />
              <input
                name="email"
                type="email"
                required
                autoComplete="email"
                placeholder="you@email.com"
                aria-label="email"
                className="h-12 rounded-pill border border-line bg-surface-page px-5 text-[16px] placeholder:text-ink-muted"
              />
              <div>
                <GlossyButton submit size="sm" disabled={state === "sending" || state === "done"}>
                  {c.waitlist.join}
                </GlossyButton>
              </div>
              <p role="status" key={state} className="pop min-h-5 text-[14px] text-ink-muted">
                {message[state]}
              </p>
            </form>
          </MacWindow>
        </div>
      </div>
    </main>
  );
}
