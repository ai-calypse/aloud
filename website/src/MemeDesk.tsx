import adam from "./assets/memes/everyone-points-at-aloud.webp";
import hand from "./assets/memes/hand-on-computer.webp";
import lose from "./assets/memes/lose-this-customer.webp";
import less from "./assets/memes/less-stress.webp";
import exit from "./assets/memes/exit-popup.webp";
import grass from "./assets/memes/touch-grass.webp";
import { MacWindow, PixelSprite } from "./components/ds";
import { Section } from "./components/layout";
import { useCopy } from "./copy";

// Memes always live in a window, with their file name as title and caption.
// ponytail: alt text is the file name until each meme gets a real description.
const memes = [
  { src: exit, name: "exit-popup.png", tilt: -2, alt: "A person pointing at a system warning that asks whether to quit" },
  { src: lose, name: "lose-this-customer.png", tilt: 1.5, alt: "A system warning on a green hill asking whether to lose this customer" },
  { src: hand, name: "hand-on-computer.png", tilt: -1, alt: "A giant hand reaching out of an old computer screen" },
  { src: grass, name: "touch-grass.png", tilt: 2, alt: "A cat on a pink television in a field, with the words touch grass" },
  { src: less, name: "less-stress.png", tilt: -1.5, alt: "A cat on a laptop on a small island, with the words less stress, more creativity" },
  { src: adam, name: "everyone-points-at-speakos.png", tilt: 1, alt: "Many people and cartoon characters pointing at each other, in the style of a famous painting" },
];

const Meme = ({ m, className }: { m: (typeof memes)[number]; className?: string }) => (
  <MacWindow
    title={m.name}
    caption={m.name}
    className={className}
    style={{ "--r": `${m.tilt}deg` } as React.CSSProperties}
  >
    <img src={m.src} alt={m.alt} loading="lazy" className="block w-full" />
  </MacWindow>
);

export default function MemeDesk() {
  const copy = useCopy();
  const [a, b, c, d, e, f] = memes;
  const tilt = "md:rotate-[var(--r)] transition-transform duration-300 ease-[var(--ease-out)] hover:rotate-0 hover:-translate-y-1.5 hover:scale-[1.02]";
  return (
    <Section>
      <div className="grid grid-cols-2 items-center gap-5 md:grid-cols-3 md:gap-x-10 md:gap-y-12">
        <Meme m={a} className={tilt} />
        <Meme m={b} className={tilt} />
        <Meme m={c} className={`${tilt} col-span-2 md:col-span-1`} />
        <Meme m={d} className={tilt} />
        <h2 className="t-section order-first col-span-2 mb-4 text-center md:order-none md:col-span-1 md:mb-0">
          {copy.memeDesk.heading}
        </h2>
        <Meme m={e} className={tilt} />
        <div className="hidden items-center justify-center gap-4 md:flex" aria-hidden="true">
          <PixelSprite name="heart" size={6} />
          <span className="font-mono text-ink-muted">^ ω ^</span>
        </div>
        <Meme m={f} className={`${tilt} col-span-2 md:col-span-1`} />
        <div className="hidden items-center justify-center gap-4 md:flex" aria-hidden="true">
          <span className="font-mono text-ink-muted">{"{ ^-^ }"}</span>
          <PixelSprite name="star" size={6} />
        </div>
      </div>
    </Section>
  );
}
