import glassesCat from "./assets/memes/glasses-cat.webp";
import listeningCat from "./assets/memes/listening-cat.webp";
import { ApprovalCard, Notch, PillLabel, PixelSprite } from "./components/ds";
import Sticker from "./components/Sticker";
import { Row, Section } from "./components/layout";
import { useCopy } from "./copy";

const littleSprites = ["mic", "heart", "star", "arrow"] as const;

export default function Agents() {
  const c = useCopy();
  return (
    <>
      <Section>
        <Row
          label={c.agents.pill}
          title={c.agents.title}
          decor={
            <Sticker rotate={7} className="absolute -bottom-[44px] right-2 z-10 w-[84px] md:-bottom-8 md:right-[6%] md:w-[112px]">
              <img src={glassesCat} alt="A cat in round glasses and a red bow tie at a laptop, reviewing the agents" loading="lazy" />
            </Sticker>
          }
          visual={
            <>
              <Notch text={c.agents.notch} />
              <ApprovalCard />
            </>
          }
        >
          <p>{c.agents.p1}</p>
          <p className="text-[13px]">{c.agents.p2}</p>
        </Row>
      </Section>

      <Section className="pt-0 md:pt-0">
        <div className="mb-6">
          <PillLabel>{c.agents.littlePill}</PillLabel>
        </div>
        <ul className="grid gap-x-8 sm:grid-cols-2 lg:grid-cols-4">
          {c.agents.little.map(([lead, text], i) => (
            <li key={lead} className="relative flex flex-col items-start gap-3 border-t border-line py-5">
              {littleSprites[i] === "heart" ? (
                <Sticker rotate={-5} className="w-[72px]">
                  <img src={listeningCat} alt="A white cat with its paws folded, captioned: i am listening" loading="lazy" />
                </Sticker>
              ) : (
                <PixelSprite name={littleSprites[i]} size={4} />
              )}
              <p>
                <span className="font-medium">{lead}</span> <span className="text-ink-muted">{text}</span>
              </p>
            </li>
          ))}
        </ul>
      </Section>
    </>
  );
}
