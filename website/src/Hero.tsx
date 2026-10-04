import { useEffect, useRef } from "react";
import hero from "./assets/hero.webp";
import { DitherEdge, GlossyButton, LogoMark } from "./components/ds";
import SmartLink from "./components/SmartLink";
import { useCopy } from "./copy";

const links = [
  ["features", "#features"],
  ["dictation", "#dictation"],
  ["commands", "#commands"],
  ["meetings", "#meetings"],
  ["pricing", "#pricing"],
  ["privacy", "/privacy"],
  ["faq", "#faq"],
] as const;

// One orchestrated load: headline lines, lede, buttons and fine print rise in 70ms apart.
// Visible by default (fill: backwards only holds them back until their own start), once.
function useIntro() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    ref.current?.querySelectorAll("[data-intro]").forEach((el, i) =>
      el.animate(
        [
          { opacity: 0, transform: "translateY(16px)", filter: "blur(4px)" },
          { opacity: 1, transform: "none", filter: "blur(0)" },
        ],
        {
          duration: 600,
          delay: 90 + i * 70,
          easing: "cubic-bezier(0.23, 1, 0.32, 1)",
          fill: "backwards",
        },
      ),
    );
  }, []);
  return ref;
}

export default function Hero() {
  const c = useCopy();
  const intro = useIntro();
  return (
    <header className="relative isolate flex min-h-[max(100svh,720px)] flex-col overflow-hidden bg-blue-700 text-neutral-0">
      {/* The scene runs from the very top; blue-700 is only a wash behind the copy
          (from the top on phones, from the left on desktop), so the boy on the
          right stays clear and white text keeps its contrast. */}
      <img
        src={hero}
        alt="Pixel art of a boy in a yellow hoodie speaking into a tin can whose string runs to a laptop, in a flower meadow beside a lake"
        width={1672}
        height={941}
        fetchPriority="high"
        className="absolute inset-x-0 bottom-0 -z-20 h-[58svh] w-full object-cover object-[100%_100%] [image-rendering:pixelated] [mask-image:linear-gradient(to_bottom,transparent,black_30%)] md:landscape:inset-0 md:landscape:h-full md:landscape:object-[60%_100%] md:landscape:[mask-image:none]"
      />
      <div className="absolute inset-0 -z-10 bg-linear-to-b from-blue-700 via-blue-700/80 via-45% to-transparent to-75% md:landscape:bg-linear-to-r md:landscape:from-blue-700 md:landscape:via-blue-700/80 md:landscape:via-35% md:landscape:to-transparent md:landscape:to-70%" />

      <nav className="mx-auto flex w-full max-w-[1180px] items-center justify-between px-5 py-5">
        <SmartLink href="/"
          className="flex items-center gap-[9px] font-head text-[26px] font-semibold leading-none tracking-[-0.03em]"
        >
          <LogoMark className="h-[22px] w-[17px]" />
          SpeakOS
        </SmartLink>
        <ul className="hidden items-center gap-8 text-[15px] md:flex">
          {links.map(([key, href]) => (
            <li key={key}>
              <SmartLink href={href} className="link-u pb-0.5">
                {c.nav[key]}
              </SmartLink>
            </li>
          ))}
        </ul>
        <GlossyButton size="sm" href="/thanks" event="download_click">
          {c.common.downloadMac}
        </GlossyButton>
      </nav>

      <div className="mx-auto flex w-full max-w-[1180px] flex-1 flex-col justify-center px-5 pb-[30svh] pt-10 md:landscape:pb-24 md:landscape:pt-0">
        <div ref={intro} className="flex max-w-[780px] flex-col items-start">
          <h1 className="t-hero">
            {c.hero.h1.map((line) => (
              <span key={line} data-intro className="block">
                {line}
              </span>
            ))}
          </h1>
          <p data-intro className="t-lede mt-6 max-w-[540px]">
            {c.hero.lede}
          </p>
          <div data-intro className="mt-8 flex flex-wrap gap-4 max-md:[&_.al-gbtn]:h-auto! max-md:[&_.al-gbtn]:min-h-14! max-md:[&_.al-gbtn]:max-w-full! max-md:[&_.al-gbtn]:whitespace-normal! max-md:[&_.al-gbtn]:py-3! max-md:[&_.al-gbtn]:text-center! max-md:[&_.al-gbtn]:text-[19px]!">
            <GlossyButton href="/thanks" icon="apple" event="download_click">
              {c.common.downloadMac}
            </GlossyButton>
            <GlossyButton href="/waitlist" tone="white" icon="windows" event="waitlist_click">
              {c.common.windowsWaitlist}
            </GlossyButton>
          </div>
          <p data-intro className="mt-4 text-[13px]">
            {c.common.macNote}
          </p>
        </div>
      </div>

      <DitherEdge />
    </header>
  );
}
