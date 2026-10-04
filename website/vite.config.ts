import { defineConfig, loadEnv, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Indexable routes. /thanks and the 404 are noindex, so they stay out of the sitemap.
const ROUTES = ["/", "/privacy", "/bugs", "/waitlist"];

// SEO plumbing: swaps __SITE_URL__ in index.html, adds the Search Console verification tag
// when GSC_TOKEN is set, and emits sitemap.xml and robots.txt at build.
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
      const urls = ROUTES.map(
        (r) =>
          `  <url><loc>${site}${r === "/" ? "/" : r}</loc><lastmod>${today}</lastmod></url>`,
      ).join("\n");
      this.emitFile({
        type: "asset",
        fileName: "sitemap.xml",
        source: `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`,
      });
      this.emitFile({
        type: "asset",
        fileName: "robots.txt",
        source: `User-agent: *\nAllow: /\nDisallow: /thanks\n\nSitemap: ${site}/sitemap.xml\n`,
      });
    },
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, ".", "");
  // ponytail: aloud.app is a guess from the releases.aloud.app hint in the copy doc; set SITE_URL.
  const site = (env.SITE_URL || "https://aloud.app").replace(/\/+$/, "");
  return {
    plugins: [react(), tailwindcss(), seo(site, env.GSC_TOKEN || "")],
    server: { port: 1430 },
  };
});
