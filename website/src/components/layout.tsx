import type { ReactNode } from "react";
import { Blip, PillLabel } from "./ds";

export const cx = (...a: (string | false | undefined)[]) => a.filter(Boolean).join(" ");

export function Section({ id, className, children }: { id?: string; className?: string; children: ReactNode }) {
  return (
    <section id={id} className={cx("px-5 py-[72px] md:py-[112px]", className)}>
      <div className="mx-auto max-w-[1180px]">{children}</div>
    </section>
  );
}

// The design system's alternating row: pill, title, short body, a blip, and a
// window opposite. `flip` swaps sides on desktop; on phones the text goes first.
export function Row({ label, title, blip, flip, visual, decor, aside, children }: { label: string; title: string; blip?: string; flip?: boolean; visual: ReactNode; decor?: ReactNode; aside?: ReactNode; children: ReactNode }) {
  return (
    <div className="relative grid items-center gap-10 md:grid-cols-2 md:gap-[72px]">
      {aside}
      <div className={cx("flex flex-col items-start gap-5", flip && "md:order-2")}>
        <PillLabel>{label}</PillLabel>
        <h2 className="t-card">{title}</h2>
        <div className="flex max-w-[480px] flex-col gap-4 text-ink-muted">{children}</div>
        {blip && <Blip>{blip}</Blip>}
      </div>
      <div className="relative flex flex-col items-center gap-4">
        {visual}
        {decor}
      </div>
    </div>
  );
}

// Hairline-separated list: a bold lead, then the sentence. Not cards.
export function Points({ items, dark }: { items: [string, string][]; dark?: boolean }) {
  return (
    <ul className="grid gap-x-12 md:grid-cols-2">
      {items.map(([lead, text]) => (
        <li key={lead} className={cx("border-t py-4", dark ? "border-neutral-0/30" : "border-line")}>
          <span className="font-medium">{lead}</span> <span className={dark ? undefined : "text-ink-muted"}>{text}</span>
        </li>
      ))}
    </ul>
  );
}
