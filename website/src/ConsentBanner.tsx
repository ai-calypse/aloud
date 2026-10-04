import { useState } from "react";
import { analyticsEnabled, getConsent, setConsent } from "./analytics";
import { GlossyButton } from "./components/ds";
import { useCopy } from "./copy";

// Shown once, only when analytics is configured. Either answer is remembered.
export default function ConsentBanner() {
  const c = useCopy().consent;
  const [open, setOpen] = useState(() => analyticsEnabled && getConsent() === null);
  if (!open) return null;
  const answer = (v: "granted" | "denied") => {
    setConsent(v);
    setOpen(false);
  };
  return (
    <div role="region" aria-label="analytics" className="pop fixed inset-x-4 bottom-20 z-40 rounded-lg border border-line bg-surface-card p-5 text-[14px] text-ink shadow-window md:bottom-4 md:left-4 md:right-auto md:max-w-[360px]">
      <p className="text-ink-muted">{c.text}</p>
      <div className="mt-4 flex gap-3">
        <GlossyButton size="sm" onClick={() => answer("granted")}>{c.accept}</GlossyButton>
        <GlossyButton size="sm" tone="white" onClick={() => answer("denied")}>{c.decline}</GlossyButton>
      </div>
    </div>
  );
}
