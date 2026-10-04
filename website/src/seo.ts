import { useEffect } from "react";

// Sets the tab title, the canonical URL and, optionally, noindex for the current page.
export function useMeta(title: string, { noindex = false }: { noindex?: boolean } = {}) {
  useEffect(() => {
    document.title = title;
    let canonical = document.querySelector<HTMLLinkElement>('link[rel="canonical"]');
    if (!canonical) {
      canonical = document.createElement("link");
      canonical.rel = "canonical";
      document.head.appendChild(canonical);
    }
    canonical.href = location.origin + location.pathname.replace(/\/+$/, "") ;
    if (!noindex) return;
    const m = document.createElement("meta");
    m.name = "robots";
    m.content = "noindex";
    document.head.appendChild(m);
    return () => m.remove();
  }, [title, noindex]);
}
