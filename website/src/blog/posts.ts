import { post as howTo } from "./posts/how-to-dictate-on-a-mac.ts";
import { post as notWorking } from "./posts/mac-dictation-not-working.ts";
import { post as shortcut } from "./posts/mac-dictation-shortcut.ts";
import { post as faster } from "./posts/is-dictation-faster-than-typing.ts";
import { post as voiceControl } from "./posts/dictation-vs-voice-control-mac.ts";
import { post as emails } from "./posts/how-to-dictate-emails-on-mac.ts";
import { post as wisprAlternatives } from "./posts/wispr-flow-alternatives.ts";
import { post as superMacWhisper } from "./posts/superwhisper-vs-macwhisper.ts";
import { post as bestDictation } from "./posts/best-dictation-software-for-mac.ts";
import { post as appleVsWispr } from "./posts/apple-dictation-vs-wispr-flow.ts";
import { post as codingVoice } from "./posts/voice-dictation-coding-mac.ts";
import { post as offlineWhisper } from "./posts/offline-whisper-dictation-mac.ts";
import { post as dictateNotes } from "./posts/how-to-dictate-notes-on-mac.ts";
import { post as adhdDyslexia } from "./posts/voice-dictation-adhd-dyslexia-mac.ts";
import { post as holdToTalk } from "./posts/hold-to-talk-vs-toggle-dictation.ts";
import { post as dragonAlt } from "./posts/nuance-dragon-mac-alternatives.ts";
import { post as slackDictation } from "./posts/how-to-dictate-slack-messages-mac.ts";

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
  esc(t)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, label, href) => `<a href="${href}"${href.startsWith("http") ? ' rel="noopener" target="_blank"' : ""}>${label}</a>`);

// Every blog CTA goes to the download page with UTMs, so GA4 splits clicks by post and by
// position (utm_campaign, utm_content), and the click event carries the same two as parameters.
export const ctaHref = (slug: string, slot: string) => `/thanks?utm_source=blog&utm_medium=cta&utm_campaign=${slug}&utm_content=${slot}`;

function ctaHtml(c: Cta, slug: string) {
  // White pill on the blue box, the blue gloss button on the white page.
  const tone = c.slot === "intro" ? "blue al-gbtn--sm" : "white";
  const a = `<a class="al-gbtn al-gbtn--${tone}" href="${esc(ctaHref(slug, c.slot))}" data-event="download_click" data-cta="${c.slot}" data-post="${slug}"><span>${esc(c.button)}</span></a>`;
  if (c.slot === "intro") return `<p class="cta-inline">${a}.</p>`;
  return `<aside class="cta cta-${c.slot}" aria-label="get SpeakOS">${c.title ? `<p class="cta-title">${esc(c.title)}</p>` : ""}${c.text ? `<p>${inline(c.text)}</p>` : ""}${a}</aside>`;
}

export const slugify = (text: string) =>
  text
    .toLowerCase()
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");

export const blockHtml = (b: Block, slug: string): string => {
  if ("h" in b) return `<h2 id="${slugify(b.h)}">${esc(b.h)}</h2>`;
  if ("p" in b) return `<p>${inline(b.p)}</p>`;
  if ("ul" in b) return `<ul>${b.ul.map((li) => `<li>${inline(li)}</li>`).join("")}</ul>`;
  if ("ol" in b) return `<ol>${b.ol.map((li) => `<li>${inline(li)}</li>`).join("")}</ol>`;
  if ("takeaways" in b) return `<aside class="takeaways"><strong>key takeaways.</strong> <ul>${b.takeaways.map((li) => `<li>${inline(li)}</li>`).join("")}</ul></aside>`;
  if ("note" in b) return `<aside class="note">${inline(b.note)}</aside>`;
  if ("svg" in b) return `<figure>${b.svg}<figcaption>${inline(b.caption)}</figcaption></figure>`;
  if ("img" in b) return `<figure><img src="${b.img}" alt="${esc(b.alt)}" width="${b.width ?? 1280}" height="${b.height ?? 720}" loading="lazy" />${b.caption ? `<figcaption>${inline(b.caption)}</figcaption>` : ""}</figure>`;
  if ("table" in b) {
    const t = b.table;
    return `<div class="tablewrap"><table><caption>${esc(t.caption)}</caption><thead><tr>${t.head.map((h) => `<th scope="col">${esc(h)}</th>`).join("")}</tr></thead><tbody>${t.rows.map((r) => `<tr>${r.map((c, i) => (i === 0 ? `<th scope="row">${inline(c)}</th>` : `<td>${inline(c)}</td>`)).join("")}</tr>`).join("")}</tbody></table></div>`;
  }
  if ("cta" in b) return ctaHtml(b.cta, slug);
  return `<section class="faq"><h2 id="frequently-asked-questions">frequently asked questions</h2>${b.faq.map(([q, a]) => `<h3>${esc(q)}</h3><p>${inline(a)}</p>`).join("")}</section>`;
};

export const extractHeadings = (p: Post) => {
  const headings: { text: string; id: string }[] = [];
  for (const b of p.blocks) {
    if ("h" in b) {
      headings.push({ text: b.h, id: slugify(b.h) });
    } else if ("faq" in b) {
      headings.push({ text: "frequently asked questions", id: "frequently-asked-questions" });
    }
  }
  return headings;
};
export function postHtml(p: Post) {
  const others = POSTS.filter((o) => o.slug !== p.slug && (!o.draft || p.draft));
  const related = others.length
    ? `<nav class="related" aria-label="more posts"><h2>keep reading</h2><div class="related-list">${others
        .slice(0, 4)
        .map(
          (o) =>
            `<article class="related-card"><a href="${postPath(o)}" class="related-card-link"><div class="related-cover-wrap"><img src="${o.cover.src}" alt="${esc(o.title)}" class="related-cover" width="320" height="180" loading="lazy" /></div><div class="related-body"><h3 class="related-title">${esc(o.title)}</h3><p class="related-desc">${esc(o.description)}</p><div class="related-meta"><span>${formatDate(o.date)}</span> · <span>${o.minutes} min read</span></div><div class="related-tags">${o.tags.slice(0, 2).map((t) => `<span class="related-tag">${esc(t)}</span>`).join("")}</div></div></a></article>`,
        )
        .join("")}</div></nav>`
    : "";
  return p.blocks.map((b) => blockHtml(b, p.slug)).join("\n") + related;
}

export const POSTS: Post[] = [
  howTo,
  notWorking,
  shortcut,
  faster,
  voiceControl,
  emails,
  wisprAlternatives,
  superMacWhisper,
  bestDictation,
  appleVsWispr,
  codingVoice,
  offlineWhisper,
  dictateNotes,
  adhdDyslexia,
  holdToTalk,
  dragonAlt,
  slackDictation,
];

export const publicPosts = () => POSTS.filter((p) => !p.draft);
export const postPath = (p: Post) => `/blog/${p.slug}`;
export const formatDate = (iso: string) => new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
