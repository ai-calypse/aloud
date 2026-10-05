// The site's two forms (windows waitlist, bug reports) post here. Each submission becomes an
// email to you through ZeptoMail; nothing is stored and nothing is sent to the visitor, so the
// endpoint cannot be used to mail strangers from our domain.
//
// Set in Vercel (Project > Settings > Environment Variables), all server-side:
//   ZEPTOMAIL_TOKEN  the Send Mail token from ZeptoMail (required; without it the forms say
//                    "not connected yet" and nothing is lost silently)
//   ZEPTOMAIL_HOST   default https://api.zeptomail.com (use .in / .eu if your account is there)
//   MAIL_FROM        default no-reply@speakos.app (must be on the verified ZeptoMail domain)
//   WAITLIST_TO      default hello@speakos.app
//   BUGS_TO          default bugs@speakos.app
declare const process: { env: Record<string, string | undefined> };

const MAX_BYTES = 10_000;
const WINDOW_MS = 10 * 60_000;
const MAX_HITS = 5;

// ponytail: per-instance memory. A cold start or a second instance resets it, so it only blunts
// casual hammering; Upstash or Vercel KV is the upgrade if the forms ever get abused.
const hits = new Map<string, number[]>();
function limited(ip: string) {
  const now = Date.now();
  const recent = (hits.get(ip) ?? []).filter((t) => now - t < WINDOW_MS);
  recent.push(now);
  hits.set(ip, recent);
  if (hits.size > 5000) hits.clear();
  return recent.length > MAX_HITS;
}

const json = (status: number, body: Record<string, unknown>) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", "Cache-Control": "no-store" } });

// Visitor text goes into an HTML email, so it is escaped, and stripped of control characters.
const esc = (s: string) => s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
const clean = (v: unknown, max: number) => (typeof v === "string" ? v.replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, "").trim().slice(0, max) : "");
const EMAIL = /^[^\s@<>"',;]{1,64}@[^\s@<>"',;]{1,190}\.[^\s@<>"',;]{2,}$/;

export async function POST(request: Request): Promise<Response> {
  const token = process.env.ZEPTOMAIL_TOKEN;
  if (!token) return json(503, { error: "not_configured" });

  // Same-origin only: the site's own pages are the only legitimate caller.
  const origin = request.headers.get("origin");
  if (origin) {
    try {
      if (new URL(origin).host !== new URL(request.url).host) return json(403, { error: "forbidden" });
    } catch {
      return json(403, { error: "forbidden" });
    }
  }
  if (!(request.headers.get("content-type") ?? "").includes("application/json")) return json(415, { error: "bad_type" });

  const ip = (request.headers.get("x-real-ip") ?? request.headers.get("x-forwarded-for") ?? "unknown").split(",")[0].trim();
  if (limited(ip)) return json(429, { error: "slow_down" });

  const raw = await request.text();
  if (raw.length > MAX_BYTES) return json(413, { error: "too_large" });
  let body: Record<string, unknown>;
  try {
    body = JSON.parse(raw);
  } catch {
    return json(400, { error: "bad_json" });
  }

  // Honeypot: a hidden field real visitors never fill. Pretend it worked.
  if (clean(body.hp, 100)) return json(200, { ok: true });

  const email = clean(body.email, 254);
  const ua = clean(request.headers.get("user-agent"), 200);
  const when = new Date().toISOString();
  let to: string, subject: string, text: string, replyTo: string | null;

  if (body.kind === "waitlist") {
    if (!EMAIL.test(email)) return json(400, { error: "bad_email" });
    to = process.env.WAITLIST_TO || "hello@speakos.app";
    subject = `Windows waitlist: ${email}`;
    text = `New Windows waitlist signup\n\nEmail: ${email}\nTime: ${when}\nBrowser: ${ua}`;
    replyTo = email;
  } else if (body.kind === "bugs") {
    const description = clean(body.description, 4000);
    if (description.length < 3) return json(400, { error: "bad_description" });
    if (email && !EMAIL.test(email)) return json(400, { error: "bad_email" });
    to = process.env.BUGS_TO || "bugs@speakos.app";
    subject = `Bug report${email ? ` from ${email}` : ""}`;
    text = `${description}\n\n---\nEmail: ${email || "(not given)"}\nmacOS: ${clean(body.macos, 40) || "(not given)"}\nTime: ${when}\nBrowser: ${ua}`;
    replyTo = email || null;
  } else {
    return json(400, { error: "bad_kind" });
  }

  const payload = {
    from: { address: process.env.MAIL_FROM || "no-reply@speakos.app", name: "SpeakOS website" },
    to: [{ email_address: { address: to } }],
    ...(replyTo ? { reply_to: [{ address: replyTo }] } : {}),
    subject,
    textbody: text,
    htmlbody: `<pre style="font:14px/1.5 -apple-system,Segoe UI,sans-serif;white-space:pre-wrap">${esc(text)}</pre>`,
  };

  try {
    const res = await fetch(`${(process.env.ZEPTOMAIL_HOST || "https://api.zeptomail.com").replace(/\/+$/, "")}/v1.1/email`, {
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
        // ZeptoMail shows the token as "Zoho-enczapikey ..."; accept it with or without the prefix.
        Authorization: /^Zoho-enczapikey /i.test(token) ? token : `Zoho-enczapikey ${token}`,
      },
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const detail = await res.text().catch(() => "");
      console.error("zeptomail rejected the message", res.status, detail.slice(0, 300));
      return json(502, { error: "send_failed" });
    }
  } catch (e) {
    console.error("zeptomail unreachable", e instanceof Error ? e.message : e);
    return json(502, { error: "send_failed" });
  }
  return json(200, { ok: true });
}
