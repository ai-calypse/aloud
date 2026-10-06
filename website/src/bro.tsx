import { useSyncExternalStore } from "react";
import { useLocation } from "react-router";
import { track } from "./analytics";

// Bro mode is which of three copy sets the site reads (see copy.ts): 0 normal, 1 bro,
// 2 bro max. It is kept in localStorage, and ?bro=0|1|2 in the URL overrides it, so a
// link can force a level. An old saved "1" still means level 1.
export type BroLevel = 0 | 1 | 2;
let level: BroLevel = 0;
const listeners = new Set<() => void>();

const parse = (v: string | null): BroLevel | null => (v === "0" || v === "1" || v === "2" ? (Number(v) as BroLevel) : null);

function commit() {
  document.documentElement.dataset.bro = String(level);
}

export function setBroLevel(v: BroLevel) {
  level = v;
  try {
    localStorage.setItem("bro", String(v));
  } catch {}
  commit();
  track("bro_mode", { level: v });
  listeners.forEach((l) => l());
}

export function startBro() {
  let saved: BroLevel | null = null;
  try {
    saved = parse(localStorage.getItem("bro"));
  } catch {}
  level = parse(new URLSearchParams(location.search).get("bro")) ?? saved ?? 0;
  commit();
}

export const useBroLevel = () => useSyncExternalStore((l) => (listeners.add(l), () => listeners.delete(l)), () => level);

const options: [BroLevel, string][] = [
  [0, "off"],
  [1, "bro"],
  [2, "bro max"],
];

export function BroToggle() {
  const { pathname } = useLocation();
  const current = useBroLevel();

  // Do not show bro mode toggle on any blog pages (/blog or /blog/*)
  if (pathname.startsWith("/blog")) {
    return null;
  }

  return (
    <div
      role="radiogroup"
      aria-label="bro mode"
      className="fixed bottom-4 right-4 z-50 flex items-center gap-2 rounded-pill border border-line bg-surface-card py-1 pl-4 pr-1 text-[13px] font-medium text-ink shadow-window"
    >
      <span>bro mode</span>
      <div className="relative flex">
        <span
          aria-hidden="true"
          className="absolute inset-y-0 left-0 w-[68px] rounded-pill bg-blue-500 shadow-chip transition-transform duration-300 ease-[var(--ease-out)]"
          style={{ transform: `translateX(${current * 100}%)` }}
        />
        {options.map(([value, label]) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={current === value}
            onClick={() => setBroLevel(value)}
            className={`relative w-[68px] rounded-pill py-1.5 text-center transition-[color,transform] duration-200 ease-out active:scale-95 ${current === value ? "text-neutral-0" : "text-ink-muted hover:text-ink"}`}
          >
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
