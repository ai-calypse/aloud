import { useEffect } from "react";

// Sets the tab title, the canonical URL and, optionally, noindex for the current page.
export function useMeta(title: string, { noindex = false, description }: { noindex?: boolean; description?: string } = {}) {
  useEffect(() => {
    document.title = title;
    let canonical = document.querySelector<HTMLLinkElement>('link[rel="canonical"]');
    if (!canonical) {
      canonical = document.createElement("link");
      canonical.rel = "canonical";
      document.head.appendChild(canonical);
    }
    canonical.href = location.origin + location.pathname.replace(/\/+$/, "") ;
    if (description) document.querySelector('meta[name="description"]')?.setAttribute("content", description);
    if (!noindex) return;
    const m = document.createElement("meta");
    m.name = "robots";
    m.content = "noindex";
    document.head.appendChild(m);
    return () => m.remove();
  }, [title, noindex, description]);
}
