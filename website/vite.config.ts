import { defineConfig, loadEnv, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Indexable routes. /thanks and the 404 are noindex, so they stay out of the sitemap.
const ROUTES = ["/", "/privacy", "/bugs", "/waitlist"];

// SEO plumbing: swaps __SITE_URL__ in index.html, adds the Search Console verification tag
// when GSC_TOKEN is set, and emits the sitemap index, the page sitemap and robots.txt at build.
function seo(site: string, gscToken: string): Plugin {
  return {
    name: "aloud-seo",
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
      const urls = ROUTES.map((r) => `  <url><loc>${site}${r}</loc><lastmod>${today}</lastmod></url>`).join("\n");
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
