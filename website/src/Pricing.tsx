import freeCat from "./assets/memes/free-cat.webp";
import proCat from "./assets/memes/pro-cat.webp";
import {
  Blip,
  GlossyButton,
  MacWindow,
  PillLabel,
  PixelSprite,
} from "./components/ds";
import { Section } from "./components/layout";
import Rise from "./components/Rise";
import Sticker from "./components/Sticker";
import { useCopy } from "./copy";

// ponytail: the plans and prices are placeholders until pricing is decided (copy doc
// open question 1). Both plans send people to the same download; upgrading is meant
// to happen inside the app, so there is no checkout here yet.
type Plan = ReturnType<typeof useCopy>["pricing"]["free"];

function Features({ items }: { items: string[] }) {
  return (
    <ul className="flex flex-col gap-3">
      {items.map((f) => (
        <li key={f} className="flex items-center gap-3">
          <PixelSprite name="check" size={3} />
          {f}
        </li>
      ))}
    </ul>
  );
}

function PlanBody({
  plan,
  children,
}: {
  plan: Pick<Plan, "name" | "price" | "per" | "blurb" | "features">;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-5 p-6 md:p-8">
      <div>
        <p className="font-medium text-ink-muted">{plan.name}</p>
        <p className="mt-1 flex items-baseline gap-3">
          <span className="font-head text-[56px] font-medium leading-none tracking-[-0.04em]">
            {plan.price}
          </span>
          <span className="text-ink-muted">{plan.per}</span>
        </p>
      </div>
      <p className="text-ink-muted">{plan.blurb}</p>
      <Features items={plan.features} />
      <div className="pt-2 max-md:[&_.al-gbtn]:h-auto! max-md:[&_.al-gbtn]:min-h-12! max-md:[&_.al-gbtn]:whitespace-normal! max-md:[&_.al-gbtn]:px-5! max-md:[&_.al-gbtn]:py-3! max-md:[&_.al-gbtn]:text-center! max-md:[&_.al-gbtn]:text-[16px]!">
        {children}
      </div>
    </div>
  );
}

export default function Pricing() {
  const c = useCopy().pricing;
  return (
    <Section id="pricing">
      <div className="mb-24 flex max-w-[640px] flex-col items-start gap-5">
        <PillLabel>{c.pill}</PillLabel>
        <h2 className="t-section">{c.title}</h2>
        <p className="t-lede text-ink-muted">{c.lede}</p>
      </div>

      <div className="mx-auto grid max-w-[900px] items-start gap-14 md:grid-cols-2 md:gap-10">
        <Rise className="relative min-w-0">
          <Sticker
            rotate={-6}
            className="absolute -top-[58px] right-4 z-10 w-[84px] md:-top-[68px] md:w-[104px]"
          >
            <img
              src={freeCat}
              alt="A cat holding out an empty wallet, because free means free"
              loading="lazy"
            />
          </Sticker>
          <MacWindow
            title={c.free.window}
            className="transition-[transform] duration-300 ease-[var(--ease-out)] hover:-translate-y-1"
          >
            <PlanBody plan={c.free}>
              <GlossyButton href="/thanks" icon="apple" event="download_click">
                {c.free.cta}
              </GlossyButton>
            </PlanBody>
          </MacWindow>
        </Rise>

        <Rise delay={110} className="relative min-w-0 md:mt-8">
          <Sticker
            rotate={6}
            className="absolute -top-[58px] right-4 z-10 w-[84px] md:-top-[68px] md:w-[104px]"
          >
            <img
              src={proCat}
              alt="A cat handing over a credit card"
              loading="lazy"
            />
          </Sticker>
          <div className="absolute -top-[46px] left-6 z-10">
            <Blip>{c.pro.blip}</Blip>
          </div>
          <MacWindow
            title={c.pro.window}
            className="transition-[transform] duration-300 ease-[var(--ease-out)] hover:-translate-y-1"
          >
            <PlanBody plan={c.pro}>
              <GlossyButton href="/thanks" tone="white" event="download_click">
                {c.pro.cta}
              </GlossyButton>
            </PlanBody>
          </MacWindow>
        </Rise>
      </div>

      <p className="mt-12 text-center text-ink-muted">{c.note}</p>
    </Section>
  );
}
