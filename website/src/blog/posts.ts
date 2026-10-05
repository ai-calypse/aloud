import { post as howTo } from "./posts/how-to-dictate-on-a-mac.ts";
import { post as notWorking } from "./posts/mac-dictation-not-working.ts";
import { post as shortcut } from "./posts/mac-dictation-shortcut.ts";
import { post as faster } from "./posts/is-dictation-faster-than-typing.ts";
import { post as voiceControl } from "./posts/dictation-vs-voice-control-mac.ts";
import { post as emails } from "./posts/how-to-dictate-emails-on-mac.ts";

// Pure data, no React: the page renders it, and vite.config.ts bakes the same HTML into static
// files at build time so crawlers that never run JavaScript still read the post.
// Text may carry [label](url) links. Everything here is first-party, so it is escaped once and
// injected as HTML by both.
export type Cta = {
  slot: "intro" | "mid" | "end";
  button: string; // first-person and specific: "get my mac typing for me"
  title?: string; // mid and end only
  text?: string;
};

export type Block =
  | { h: string }
  | { p: string }
  | { ul: string[] }
  | { ol: string[] }
  | { takeaways: string[] }
  | { note: string }
  | { svg: string; caption: string }
  | { img: string; alt: string; caption?: string; width?: number; height?: number }
  | { table: { caption: string; head: string[]; rows: string[][] } }
  | { cta: Cta }
  | { faq: [string, string][] };

export type Post = {
  slug: string;
  title: string; // the H1
  metaTitle: string; // the <title> tag, 40-60 characters, keyword first
  description: string; // 150-160 characters, one statistic
  date: string; // YYYY-MM-DD
  updated?: string;
  checked?: string; // the day every cited source was last opened and compared, shown under the byline
  minutes: number;
  tags: string[];
  cover: { src: string; alt: string }; // a file under public/, so the URL is stable for og:image
  // A draft is served but noindex and left out of the sitemap and the blog index.
  draft?: boolean;
  blocks: Block[];
};

// Swap in a named person (name, jobTitle, a bio on /about) and the byline, schema and E-E-A-T
// signals all follow. Until then the author is the team.
export const AUTHOR: { name: string; url: string; person?: { jobTitle: string; sameAs?: string[] } } = { name: "The SpeakOS team", url: "/about" };

const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const inline = (t: string) =>
  esc(t).replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, label, href) => `<a href="${href}"${href.startsWith("http") ? ' rel="noopener" target="_blank"' : ""}>${label}</a>`);

// Every blog CTA goes to the download page with UTMs, so GA4 splits clicks by post and by
// position (utm_campaign, utm_content), and the click event carries the same two as parameters.
export const ctaHref = (slug: string, slot: string) => `/thanks?utm_source=blog&utm_medium=cta&utm_campaign=${slug}&utm_content=${slot}`;

function ctaHtml(c: Cta, slug: string) {
  // White pill on the blue box, the blue gloss button on the white page.
  const tone = c.slot === "intro" ? "blue al-gbtn--sm" : "white";
  const a = `<a class="al-gbtn al-gbtn--${tone}" href="${esc(ctaHref(slug, c.slot))}" data-event="download_click" data-cta="${c.slot}" data-post="${slug}"><span>${esc(c.button)}</span></a>`;
  if (c.slot === "intro") return `<p class="cta-inline">${a}</p>`;
  return `<aside class="cta cta-${c.slot}" aria-label="get SpeakOS">${c.title ? `<p class="cta-title">${esc(c.title)}</p>` : ""}${c.text ? `<p>${inline(c.text)}</p>` : ""}${a}</aside>`;
}

export const blockHtml = (b: Block, slug: string): string => {
  if ("h" in b) return `<h2>${esc(b.h)}</h2>`;
  if ("p" in b) return `<p>${inline(b.p)}</p>`;
  if ("ul" in b) return `<ul>${b.ul.map((li) => `<li>${inline(li)}</li>`).join("")}</ul>`;
  if ("ol" in b) return `<ol>${b.ol.map((li) => `<li>${inline(li)}</li>`).join("")}</ol>`;
  if ("takeaways" in b) return `<aside class="takeaways"><strong>key takeaways</strong><ul>${b.takeaways.map((li) => `<li>${inline(li)}</li>`).join("")}</ul></aside>`;
  if ("note" in b) return `<aside class="note">${inline(b.note)}</aside>`;
  if ("svg" in b) return `<figure>${b.svg}<figcaption>${inline(b.caption)}</figcaption></figure>`;
  if ("img" in b) return `<figure><img src="${b.img}" alt="${esc(b.alt)}" width="${b.width ?? 1280}" height="${b.height ?? 720}" loading="lazy" />${b.caption ? `<figcaption>${inline(b.caption)}</figcaption>` : ""}</figure>`;
  if ("table" in b) {
    const t = b.table;
    return `<div class="tablewrap"><table><caption>${esc(t.caption)}</caption><thead><tr>${t.head.map((h) => `<th scope="col">${esc(h)}</th>`).join("")}</tr></thead><tbody>${t.rows.map((r) => `<tr>${r.map((c, i) => (i === 0 ? `<th scope="row">${inline(c)}</th>` : `<td>${inline(c)}</td>`)).join("")}</tr>`).join("")}</tbody></table></div>`;
  }
  if ("cta" in b) return ctaHtml(b.cta, slug);
  return `<section class="faq"><h2>frequently asked questions</h2>${b.faq.map(([q, a]) => `<h3>${esc(q)}</h3><p>${inline(a)}</p>`).join("")}</section>`;
};

// The body, plus a "keep reading" list so every post links to the others.
export function postHtml(p: Post) {
  const others = POSTS.filter((o) => o.slug !== p.slug && (!o.draft || p.draft));
  const related = others.length
    ? `<nav class="related" aria-label="more posts"><h2>keep reading</h2><ul>${others.map((o) => `<li><a href="${postPath(o)}">${esc(o.title)}</a></li>`).join("")}</ul></nav>`
    : "";
  return p.blocks.map((b) => blockHtml(b, p.slug)).join("\n") + related;
}

export const POSTS: Post[] = [howTo, notWorking, shortcut, faster, voiceControl, emails];

export const publicPosts = () => POSTS.filter((p) => !p.draft);
export const postPath = (p: Post) => `/blog/${p.slug}`;
export const formatDate = (iso: string) => new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
