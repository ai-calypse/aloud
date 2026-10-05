// Layered, free, consent-first measurement:
//  - Google Analytics 4 (VITE_GA_ID): visitors, sources, conversions. Consent Mode starts
//    everything denied; "accept" upgrades analytics_storage only. Ads signals stay off.
//  - Microsoft Clarity (VITE_CLARITY_ID): heatmaps and session replays. Its script is not
//    even downloaded until the visitor says yes.
//  - Google Search Console: verification tag and sitemap, handled in vite.config.ts.
// Nothing loads without its ID, and Do Not Track switches all of it off.
declare global {
  interface Window {
    dataLayer: unknown[];
    gtag?: (...args: unknown[]) => void;
  }
}

// The GA4 measurement id and the Clarity project id are public (both appear in any page that
// uses them), so they ship as the defaults for the live site. They only apply on speakos.app in a
// production build, so local dev, previews and tests never count as visits. VITE_GA_ID and
// VITE_CLARITY_ID override them.
const LIVE_GA_ID = "G-XGB4T9TPTB";
const onLiveSite = import.meta.env.PROD && /(^|\.)speakos\.app$/.test(location.hostname);
const ID = (import.meta.env.VITE_GA_ID as string | undefined) || (onLiveSite ? LIVE_GA_ID : undefined);
const LIVE_CLARITY_ID = "yspzx5k66a";
const CLARITY = (import.meta.env.VITE_CLARITY_ID as string | undefined) || (onLiveSite ? LIVE_CLARITY_ID : undefined);
const KEY = "consent";
const dnt =
  typeof navigator !== "undefined" &&
  (navigator.doNotTrack === "1" ||
    (window as { doNotTrack?: string }).doNotTrack === "1");

export const analyticsEnabled = (Boolean(ID) || Boolean(CLARITY)) && !dnt;

export const getConsent = (): "granted" | "denied" | null => {
  try {
    const v = localStorage.getItem(KEY);
    return v === "granted" || v === "denied" ? v : null;
  } catch {
    return null;
  }
};

type ClarityFn = ((...args: unknown[]) => void) & { q?: unknown[] };
let clarityLoaded = false;
function loadClarity() {
  if (!CLARITY || dnt || clarityLoaded) return;
  clarityLoaded = true;
  const w = window as unknown as { clarity?: ClarityFn };
  // oxlint-disable-next-line prefer-rest-params
  w.clarity =
    w.clarity ||
    function clarity() {
      (w.clarity!.q = w.clarity!.q || []).push(arguments);
    };
  const s = document.createElement("script");
  s.async = true;
  s.src = `https://www.clarity.ms/tag/${encodeURIComponent(CLARITY)}`;
  document.head.appendChild(s);
}

export function initAnalytics() {
  if (!analyticsEnabled) return;
  if (getConsent() === "granted") loadClarity();
  if (!ID) return;
  window.dataLayer = window.dataLayer || [];
  // gtag.js wants the real `arguments` object, not an array.
  // oxlint-disable-next-line prefer-rest-params
  window.gtag = function gtag() {
    window.dataLayer.push(arguments);
  };
  window.gtag("consent", "default", {
    ad_storage: "denied",
    ad_user_data: "denied",
    ad_personalization: "denied",
    analytics_storage: getConsent() === "granted" ? "granted" : "denied",
    wait_for_update: 500,
  });
  window.gtag("js", new Date());
  // page_view is sent by the router on every navigation, so the automatic one is off.
  window.gtag("config", ID, {
    send_page_view: false,
    allow_google_signals: false,
    allow_ad_personalization_signals: false,
  });
  const s = document.createElement("script");
  s.async = true;
  s.src = `https://www.googletagmanager.com/gtag/js?id=${encodeURIComponent(ID)}`;
  document.head.appendChild(s);

  // Any element with data-event="name" reports that event when clicked.
  document.addEventListener("click", (e) => {
    const el = (e.target as Element).closest<HTMLElement>("[data-event]");
    if (el)
      track(el.dataset.event!, {
        link_text: el.textContent?.trim().slice(0, 60),
        page_path: location.pathname,
      });
  });
  watchScroll();
}

export function setConsent(v: "granted" | "denied") {
  try {
    localStorage.setItem(KEY, v);
  } catch {}
  window.gtag?.("consent", "update", { analytics_storage: v });
  if (v === "granted") loadClarity();
}

// GA4's own `scroll` event fires once, at 90%. These quarter marks show where people stop.
// Pages that fit on one screen report nothing, so the waitlist form is not "100% read".
const MARKS = [25, 50, 75, 100];
let reached = new Set<number>();
function watchScroll() {
  let queued = false;
  const check = () => {
    queued = false;
    const el = document.documentElement;
    if (el.scrollHeight < innerHeight * 1.2) return;
    const pct = ((scrollY + innerHeight) / el.scrollHeight) * 100;
    for (const m of MARKS)
      if (pct >= m - 1 && !reached.has(m)) {
        reached.add(m);
        track("scroll_depth", { percent: m, page_path: location.pathname });
      }
  };
  addEventListener(
    "scroll",
    () => {
      if (!queued) {
        queued = true;
        requestAnimationFrame(check);
      }
    },
    { passive: true },
  );
}

export const pageView = (path: string) => {
  reached = new Set();
  window.gtag?.("event", "page_view", {
    page_path: path,
    page_title: document.title,
    page_location: location.href,
  });
};

export function track(name: string, params?: Record<string, unknown>) {
  window.gtag?.("event", name, params);
  (window as unknown as { clarity?: ClarityFn }).clarity?.("event", name);
}
