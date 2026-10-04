import integrated from "./assets/integrated.webp";
import { Blip, DitherEdge } from "./components/ds";
import { useCopy } from "./copy";

const bubble = (t: string) => (
  <Blip className="max-w-[300px] whitespace-normal text-left leading-snug">{t}</Blip>
);

// The picture is the message: two armfuls of app icons. The bubbles name what is in each pile.
// The dither edges dissolve it into the page at both ends, like the hero.
export default function Apps() {
  const c = useCopy();
  const bubbles = [c.apps.mac, c.apps.services];
  return (
    <section id="apps" className="pb-[40px] pt-[72px] md:pb-[72px] md:pt-[112px]">
      <div className="mx-auto mb-6 max-w-[760px] px-5 text-center md:mb-4">
        <h2 className="t-section">{c.apps.title}</h2>
        <p className="t-lede mt-4 text-ink-muted">{c.apps.lede}</p>
      </div>

      <div className="relative">
        <img
          src={integrated}
          alt="Two cows with pink bows in a sunset field, looking on as a stick figure hugs two big piles of app icons"
          width={1448}
          height={1086}
          loading="lazy"
          className="block aspect-[4/5] w-full object-cover object-[50%_60%] md:aspect-[16/9]"
        />
        <DitherEdge position="top" />
        <DitherEdge />
        <div className="absolute bottom-[12%] left-[19%] hidden md:block">{bubble(bubbles[0])}</div>
        <div className="absolute bottom-[12%] right-[19%] hidden md:block">{bubble(bubbles[1])}</div>
      </div>

      <div className="flex flex-col items-center gap-3 px-5 md:hidden">
        {bubbles.map((t) => (
          <div key={t}>{bubble(t)}</div>
        ))}
      </div>
    </section>
  );
}
