import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { defineConfig, loadEnv, type Plugin, type ResolvedConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { AUTHOR, POSTS, postHtml, postPath, publicPosts } from "./src/blog/posts.ts";
import { ABOUT, aboutHtml } from "./src/blog/about.ts";

// Indexable routes. /thanks and the 404 are noindex, so they stay out of the sitemap.
const ROUTES = ["/", "/privacy", "/bugs", "/waitlist"];

const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// The app is client-rendered, so a crawler that does not run JavaScript sees an empty page.
// Blog pages get their real text, title and description written into static HTML at build time;
// React replaces it the moment it boots. ponytail: only /blog is prerendered, the other routes
// still depend on the crawler running JS. Prerendering the rest means SSR-safe components.
type Static = { path: string; title: string; description: string; body: string; noindex: boolean; image?: string; imageAlt?: string; published?: string; modified?: string; ld?: object[] };
function staticPages(site: string): Static[] {
  const pub = publicPosts();
  const list = POSTS.map((p): Static => ({
    path: postPath(p),
    title: p.metaTitle,
    description: p.description,
    noindex: Boolean(p.draft),
    body: `<article><h1>${esc(p.title)}</h1><p>by <a href="${AUTHOR.url}">${esc(AUTHOR.name)}</a>, ${p.date}${p.checked ? `. sources last checked ${p.checked} (<a href="/about">how we choose sources</a>).` : ""}</p><img src="${p.cover.src}" alt="${esc(p.cover.alt)}" width="1280" height="720" fetchpriority="high" decoding="async" />${postHtml(p)}</article>`,
    image: `${site}${p.cover.src.replace(/\.webp$/, "-og.jpg")}`,
    imageAlt: p.cover.alt,
    published: p.date,
    modified: p.updated ?? p.checked ?? p.date,
    ld: [
      {
        "@context": "https://schema.org",
        "@type": "BlogPosting",
        headline: p.title.replace(/\.$/, ""),
        description: p.description,
        datePublished: p.date,
        dateModified: p.updated ?? p.checked ?? p.date,
        inLanguage: "en",
        keywords: p.tags.join(", "),
        wordCount: postHtml(p).replace(/<svg[\s\S]*?<\/svg>/g, " ").replace(/<[^>]+>/g, " ").split(/\s+/).filter(Boolean).length,
        timeRequired: `PT${p.minutes}M`,
        isAccessibleForFree: true,
        author: AUTHOR.person
          ? { "@type": "Person", name: AUTHOR.name, url: `${site}${AUTHOR.url}`, jobTitle: AUTHOR.person.jobTitle, ...(AUTHOR.person.sameAs ? { sameAs: AUTHOR.person.sameAs } : {}) }
          : { "@type": "Organization", name: AUTHOR.name, url: `${site}${AUTHOR.url}` },
        publisher: { "@type": "Organization", name: "SpeakOS", url: site, logo: { "@type": "ImageObject", url: `${site}/favicon.svg` } },
        image: { "@type": "ImageObject", url: `${site}${p.cover.src}`, width: 1280, height: 720 },
        mainEntityOfPage: `${site}${postPath(p)}`,
        // The primary sources the post cites, so a reader (or a model) can follow the evidence.
        citation: [...new Set([...postHtml(p).matchAll(/href="(https?:[^"]+)"/g)].map((m) => m[1].replace(/&amp;/g, "&")))],
      },
      {
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        itemListElement: [
          { "@type": "ListItem", position: 1, name: "SpeakOS", item: site },
          { "@type": "ListItem", position: 2, name: "Blog", item: `${site}/blog` },
          { "@type": "ListItem", position: 3, name: p.title.replace(/\.$/, ""), item: `${site}${postPath(p)}` },
        ],
      },
      ...p.blocks.flatMap((b) =>
        "faq" in b
          ? [{ "@context": "https://schema.org", "@type": "FAQPage", mainEntity: b.faq.map(([q, a]) => ({ "@type": "Question", name: q, acceptedAnswer: { "@type": "Answer", text: a.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1") } })) }]
          : [],
      ),
    ],
  }));
  const index: Static = {
    path: "/blog",
    title: "blog · SpeakOS",
    description: "Short posts on talking to your mac.",
    noindex: pub.length === 0,
    body: `<h1>blog</h1><ul>${pub.map((p) => `<li><a href="${postPath(p)}">${esc(p.title)}</a> ${esc(p.description)}</li>`).join("")}</ul>`,
  };
  const about: Static = { path: "/about", title: ABOUT.metaTitle, description: ABOUT.description, noindex: false, body: `<article><h1>${esc(ABOUT.title)}</h1>${aboutHtml()}</article>` };
  return [index, about, ...list];
}

// Rewrites the built index.html's head and root for one static page.
function renderStatic(shell: string, site: string, pg: Static) {
  const url = `${site}${pg.path}`;
  const meta = (html: string, key: string, value: string) =>
    html.replace(new RegExp(`(<meta[^>]*?(?:name|property)="${key}"[^>]*?content=")[^"]*(")`), (_m, a, b) => `${a}${esc(value)}${b}`);
  let html = shell.replace(/<title>[\s\S]*?<\/title>/, `<title>${esc(pg.title)}</title>`);
  for (const k of ["description", "og:description", "twitter:description"]) html = meta(html, k, pg.description);
  for (const k of ["og:title", "twitter:title"]) html = meta(html, k, pg.title);
  html = meta(html, "og:url", url);
  html = meta(html, "og:type", pg.ld ? "article" : "website");
  if (pg.image) {
    for (const k of ["og:image", "twitter:image"]) html = meta(html, k, pg.image);
    // blog posts share a 1200x630 JPG crop of the cover, the format social crawlers handle best
    html = meta(html, "og:image:width", "1200");
    html = meta(html, "og:image:height", "630");
  }
  const extraMeta = [
    pg.imageAlt ? `<meta property="og:image:alt" content="${esc(pg.imageAlt)}" />` : "",
    pg.published ? `<meta property="article:published_time" content="${pg.published}" />` : "",
    pg.modified ? `<meta property="article:modified_time" content="${pg.modified}" />` : "",
  ].filter(Boolean);
  const head = [
    `<link rel="canonical" href="${url}" />`,
    ...extraMeta,
    pg.noindex ? `<meta name="robots" content="noindex" />` : "",
    ...(pg.ld ?? []).map((o) => `<script type="application/ld+json">${JSON.stringify(o).replace(/</g, "\\u003c")}</script>`),
    `<style>.static-copy{position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden}</style>`,
    `<noscript><style>.static-copy{position:static;width:auto;height:auto;max-width:720px;margin:0 auto;padding:24px;font-family:system-ui,sans-serif}</style></noscript>`,
  ].join("\n    ");
  return html.replace("</head>", `    ${head}\n  </head>`).replace('<div id="root"></div>', `<div id="root"><main class="static-copy">${pg.body}</main></div>`);
}

// SEO plumbing: swaps __SITE_URL__ in index.html, adds the Search Console verification tag
// when GSC_TOKEN is set, and emits the sitemap index, the page sitemap and robots.txt at build.
function seo(site: string, gscToken: string): Plugin {
  let cfg: ResolvedConfig;
  return {
    name: "aloud-seo",
    configResolved: (c) => void (cfg = c),
    closeBundle() {
      if (cfg.command !== "build") return;
      const dist = resolve(cfg.root, cfg.build.outDir);
      const shell = readFileSync(join(dist, "index.html"), "utf8");
      for (const pg of staticPages(site)) {
        mkdirSync(join(dist, pg.path), { recursive: true });
        writeFileSync(join(dist, pg.path, "index.html"), renderStatic(shell, site, pg));
      }
    },
    transformIndexHtml: {
      order: "pre",
      handler: (html) => ({
        html: html.split("__SITE_URL__").join(site),
        tags: gscToken
          ? [
              {
                tag: "meta",
                attrs: { name: "google-site-verification", content: gscToken },
                injectTo: "head",
              },
            ]
          : [],
      }),
    },
    generateBundle() {
      const today = new Date().toISOString().slice(0, 10);
      const xml = (body: string) => `<?xml version="1.0" encoding="UTF-8"?>\n${body}\n`;
      const pub = publicPosts();
      const paths = [...ROUTES, "/about", ...(pub.length ? ["/blog", ...pub.map(postPath)] : [])];
      const urls = paths.map((r) => `  <url><loc>${site}${r}</loc><lastmod>${today}</lastmod></url>`).join("\n");
      const pages = xml(`<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>`);
      // The index points at sitemap-0.xml (the real list). sitemap.xml is kept as a copy so an
      // earlier submission of that address keeps working.
      const index = xml(`<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n  <sitemap><loc>${site}/sitemap-0.xml</loc><lastmod>${today}</lastmod></sitemap>\n</sitemapindex>`);
      for (const [fileName, source] of [["sitemap-0.xml", pages], ["sitemap.xml", pages], ["sitemap-index.xml", index]] as const) {
        this.emitFile({ type: "asset", fileName, source });
      }
      const pubList = publicPosts().map((p) => `- [${p.title.replace(/\.$/, "")}](${site}${postPath(p)}): ${p.description}`).join("\n");
      const llms = `# SpeakOS

> SpeakOS is voice dictation and voice commands for the Mac. Hold a key and speak, and clean text lands in whatever app has focus. Say what you want done (reminders, messages, files, music, connected apps) and SpeakOS does it, showing each action before it happens. Dictation runs on the Mac by default and works offline. A Windows version is on a waitlist.

## Pages

- [Home](${site}/): what SpeakOS does, with real commands
- [Privacy](${site}/privacy): what stays on the Mac and what goes online
- [About and editorial policy](${site}/about): who writes the blog and how sources are chosen
- [Windows waitlist](${site}/waitlist): sign up for the Windows version
- [Report a bug or a correction](${site}/bugs)
${pubList ? `\n## Blog\n\n${pubList}\n` : ""}`;
      this.emitFile({ type: "asset", fileName: "llms.txt", source: llms });
      this.emitFile({ type: "asset", fileName: "robots.txt", source: `User-agent: *\nAllow: /\nDisallow: /thanks\n\nSitemap: ${site}/sitemap-index.xml\n` });
    },
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  // ponytail: the default is the owned domain; set SITE_URL if it ever differs (preview deploys).
  const site = (env.SITE_URL || "https://speakos.app").replace(/\/+$/, "");
  return {
    plugins: [react(), tailwindcss(), seo(site, env.GSC_TOKEN || "")],
    server: { port: 1430 },
  };
});
