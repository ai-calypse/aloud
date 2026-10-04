import { useId, useState } from "react";
import faqPoster from "./assets/memes/faq-poster.webp";
import faqVideo from "./assets/memes/faq.mp4";
import readingCat from "./assets/memes/reading-cat.webp";
import { PillLabel } from "./components/ds";
import MemeVideo from "./components/MemeVideo";
import { Section } from "./components/layout";
import Sticker from "./components/Sticker";
import { track } from "./analytics";
import { useCopy } from "./copy";

// One question. Opening lifts it into a card, the plus turns into a cross, and the
// answer unrolls (grid rows 0fr to 1fr) while it fades up. Closed answers are inert so
// they cannot be tabbed into.
function Item({ q, a }: { q: string; a: string }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div
      className={`-mx-5 rounded-[16px] border px-5 transition-[background-color,box-shadow,border-color] duration-300 ease-[var(--ease-out)] ${
        open ? "border-transparent bg-surface-card shadow-window" : "border-x-transparent border-b-transparent border-t-line"
      }`}
    >
      <h3>
        <button
          type="button"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => {
            if (!open) track("faq_open", { question: q });
            setOpen(!open);
          }}
          className="group flex w-full items-center justify-between gap-6 py-5 text-left font-head text-[20px] font-medium tracking-[-0.02em] active:scale-[0.99] transition-transform duration-150 ease-out"
        >
          <span className="transition-transform duration-200 ease-[var(--ease-out)] group-hover:translate-x-1">{q}</span>
          <span
            aria-hidden="true"
            className={`flex size-8 flex-none items-center justify-center rounded-full transition-[transform,background-color,color] duration-300 ease-[var(--ease-out)] ${
              open ? "rotate-45 bg-blue-500 text-neutral-0" : "bg-surface-page text-ink-muted group-hover:bg-blue-100"
            }`}
          >
            <svg viewBox="0 0 16 16" className="size-4" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <path d="M8 2.5v11M2.5 8h11" />
            </svg>
          </span>
        </button>
      </h3>
      <div id={id} role="region" inert={!open} className={`grid transition-[grid-template-rows] duration-300 ease-[var(--ease-out)] ${open ? "grid-rows-[1fr]" : "grid-rows-[0fr]"}`}>
        <div className="overflow-hidden">
          <p className={`max-w-[640px] pb-5 text-ink-muted transition-[opacity,transform] duration-300 ease-[var(--ease-out)] ${open ? "translate-y-0 opacity-100 delay-75" : "-translate-y-1 opacity-0"}`}>{a}</p>
        </div>
      </div>
    </div>
  );
}

export default function Faq() {
  const c = useCopy();
  return (
    <Section id="faq">
      <div className="relative mx-auto max-w-[760px]">
        {/* in the empty space above the list, level with the pill */}
        <Sticker rotate={6} className="absolute -top-[84px] right-2 w-[104px] md:right-6 md:w-[130px]">
          <MemeVideo src={faqVideo} poster={faqPoster} alt="A man in a headset resting his chin on his hand, thinking" />
        </Sticker>
        {/* the curious cat: in the left gutter on wide screens, below the list otherwise */}
        <Sticker rotate={-6} className="absolute -bottom-[120px] left-2 w-[96px] min-[1100px]:-left-[150px] min-[1100px]:bottom-auto min-[1100px]:top-[42%] min-[1100px]:w-[130px]">
          <img src={readingCat} alt="A close-up of a ginger cat with a question mark above its head" loading="lazy" />
        </Sticker>
        <div className="mb-6">
          <PillLabel>{c.faq.pill}</PillLabel>
        </div>
        {c.faq.items.map(([q, a]) => (
          <Item key={q} q={q} a={a} />
        ))}
      </div>
    </Section>
  );
}
