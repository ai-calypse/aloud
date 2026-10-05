import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { defineConfig, loadEnv, type Plugin, type ResolvedConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { POSTS, blocksHtml, postPath, publicPosts } from "./src/blog/posts.ts";

// Indexable routes. /thanks and the 404 are noindex, so they stay out of the sitemap.
const ROUTES = ["/", "/privacy", "/bugs", "/waitlist"];

const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// The app is client-rendered, so a crawler that does not run JavaScript sees an empty page.
// Blog pages get their real text, title and description written into static HTML at build time;
// React replaces it the moment it boots. ponytail: only /blog is prerendered, the other routes
// still depend on the crawler running JS. Prerendering the rest means SSR-safe components.
type Static = { path: string; title: string; description: string; body: string; noindex: boolean; image?: string; ld?: object[] };
function staticPages(site: string): Static[] {
  const pub = publicPosts();
  const list = POSTS.map((p): Static => ({
    path: postPath(p),
    title: `${p.title.replace(/\.$/, "")} · SpeakOS`,
    description: p.description,
    noindex: Boolean(p.draft),
    body: `<article><h1>${esc(p.title)}</h1><p>${p.date}</p>${p.cover ? `<img src="${p.cover.src}" alt="${esc(p.cover.alt)}" />` : ""}${blocksHtml(p.blocks)}</article>`,
    image: p.cover ? `${site}${p.cover.src}` : undefined,
    ld: [
      {
        "@context": "https://schema.org",
        "@type": "BlogPosting",
        headline: p.title.replace(/\.$/, ""),
        description: p.description,
        datePublished: p.date,
        author: { "@type": "Organization", name: "SpeakOS", url: site },
        image: p.cover ? `${site}${p.cover.src}` : `${site}/og.jpg`,
        mainEntityOfPage: `${site}${postPath(p)}`,
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
  return [index, ...list];
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
  if (pg.image) for (const k of ["og:image", "twitter:image"]) html = meta(html, k, pg.image);
  const head = [
    `<link rel="canonical" href="${url}" />`,
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
      const paths = [...ROUTES, ...(pub.length ? ["/blog", ...pub.map(postPath)] : [])];
      const urls = paths.map((r) => `  <url><loc>${site}${r}</loc><lastmod>${today}</lastmod></url>`).join("\n");
      const pages = xml(`<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>`);
      // The index points at sitemap-0.xml (the real list). sitemap.xml is kept as a copy so an
      // earlier submission of that address keeps working.
      const index = xml(`<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n  <sitemap><loc>${site}/sitemap-0.xml</loc><lastmod>${today}</lastmod></sitemap>\n</sitemapindex>`);
      for (const [fileName, source] of [["sitemap-0.xml", pages], ["sitemap.xml", pages], ["sitemap-index.xml", index]] as const) {
        this.emitFile({ type: "asset", fileName, source });
      }
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
