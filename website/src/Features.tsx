import featuresPoster from "./assets/memes/features-poster.webp";
import featuresVideo from "./assets/memes/features.mp4";
import peak from "./assets/memes/peak-features.webp";
import writeCat from "./assets/memes/write-cat.webp";
import { GlossyButton, MacWindow, Notch, PillLabel } from "./components/ds";
import MemeVideo from "./components/MemeVideo";
import Sticker from "./components/Sticker";
import { Points, Row, Section } from "./components/layout";
import { useCopy } from "./copy";
import { useTyped } from "./components/useTyped";

// The "say it" window: the example commands type themselves out, one after another.
function TypedCommands({ lines }: { lines: string[] }) {
  const all = lines.join("\n");
  const [ref, n] = useTyped<HTMLUListElement>(all, { speed: 14, start: 300 });
  let used = 0;
  return (
    <ul
      ref={ref}
      className="px-4 py-2 text-[15px]"
      aria-label={lines.join(" ")}
    >
      {lines.map((line) => {
        const shown = Math.max(0, Math.min(line.length, n - used));
        const typing = shown > 0 && shown < line.length;
        used += line.length + 1;
        return (
          <li
            key={line}
            aria-hidden="true"
            className="border-b border-line py-2.5 last:border-0"
          >
            <span className="grid">
              <span className="invisible col-start-1 row-start-1">
                “{line}”
              </span>
              <span
                className={`col-start-1 row-start-1 ${typing ? "typing-caret" : ""}`}
              >
                {shown > 0
                  ? `“${line.slice(0, shown)}${shown === line.length ? "”" : ""}`
                  : ""}
              </span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}

const Field = ({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) => (
  <div className="flex gap-3 border-b border-line px-4 py-2.5 text-[14px]">
    <span className="w-14 flex-none text-ink-muted">{label}</span>
    <span>{children}</span>
  </div>
);

export default function Features() {
  const c = useCopy();
  return (
    <>
      <Section
        id="features"
        className="pb-[48px] pt-[24px] md:pb-[72px] md:pt-[48px]"
      >
        <div className="flex items-end gap-6 md:gap-10">
          <div className="flex max-w-[640px] flex-col items-start gap-5">
            <PillLabel>{c.features.pill}</PillLabel>
            <h2 className="t-section">{c.features.title}</h2>
            <p className="t-lede text-ink-muted">{c.features.lede}</p>
          </div>
          <Sticker rotate={6} className="mb-1 w-[96px] flex-none md:w-[150px]">
            <img
              src={peak}
              alt="A basketball player looking at a burning piece of paper"
              loading="lazy"
            />
          </Sticker>
        </div>
      </Section>

      <Section id="dictation" className="pt-0 md:pt-0">
        <Row
          label={c.dictation.pill}
          title={c.dictation.title}
          blip={c.dictation.blip}
          decor={
            <Sticker
              rotate={7}
              className="absolute -bottom-[64px] right-1 z-10 w-[76px] md:bottom-auto md:right-0 md:top-0 md:w-[130px]"
            >
              <MemeVideo
                src={featuresVideo}
                poster={featuresPoster}
                alt="A lego man in a white tuxedo at a party, smiling at the camera"
              />
            </Sticker>
          }
          visual={
            <>
              <Notch text={c.dictation.blip} />
              <MacWindow title="mail" className="w-full max-w-[440px]">
                <Field label="to">priya</Field>
                <Field label="subject">this week</Field>
                <p className="px-4 py-4 text-[15px]">
                  hi priya, wednesday works for me.
                </p>
              </MacWindow>
            </>
          }
        >
          <p>{c.dictation.p1}</p>
          <p>{c.dictation.p2}</p>
        </Row>
        <div className="mt-[72px]">
          <Points items={c.dictation.points} />
          <p className="border-t border-line pt-4 text-ink-muted">
            {c.dictation.escape}
          </p>
        </div>
      </Section>

      <Section id="commands" className="pt-0 md:pt-0">
        <Row
          label={c.commands.pill}
          title={c.commands.title}
          blip={c.commands.blip}
          flip
          visual={
            <MacWindow title="say it" className="w-full max-w-[440px]">
              <TypedCommands lines={c.commands.examples} />
            </MacWindow>
          }
        >
          <p>{c.commands.p}</p>
        </Row>
        <div className="mt-[72px]">
          <Points items={c.commands.more} />
        </div>
      </Section>

      <Section className="pt-0 md:pt-0">
        <Row
          label={c.approve.pill}
          title={c.approve.title}
          blip={c.approve.blip}
          visual={
            <MacWindow title="SpeakOS" className="w-full max-w-[440px]">
              <Field label="to">#team</Field>
              <Field label="slack">hey team, shipping at five.</Field>
              <div className="flex items-center gap-3 px-4 py-4">
                <GlossyButton size="sm">{c.approve.send}</GlossyButton>
                <GlossyButton size="sm" tone="white">
                  {c.approve.cancel}
                </GlossyButton>
                <span className="text-[13px] text-ink-muted">
                  {c.approve.countdown}
                </span>
              </div>
            </MacWindow>
          }
        >
          <p>{c.approve.p}</p>
        </Row>
        <div className="mt-[72px]">
          <Points items={c.approve.points} />
        </div>
      </Section>

      <Section id="meetings" className="pt-0 md:pt-0">
        <Row
          label={c.meetings.pill}
          title={c.meetings.title}
          flip
          decor={
            <Sticker
              rotate={6}
              className="absolute -top-[60px] right-0 z-10 w-[84px] md:-top-[88px] md:right-4 md:w-[112px]"
            >
              <img
                src={writeCat}
                alt="A ginger cat typing notes on a laptop"
                loading="lazy"
              />
            </Sticker>
          }
          visual={
            <MacWindow title="weekly sync.md" className="w-full max-w-[440px]">
              <div className="flex flex-col gap-3 px-4 py-4 text-[15px]">
                {[
                  "launch moves to friday.",
                  "docs are ready; pricing is open.",
                  "ship the beta to ten people first.",
                  "ana sends the invite list.",
                ].map((body, i) => (
                  <div key={body}>
                    <div className="text-[12px] font-medium text-ink-muted">
                      {c.meetings.sections[i]}
                    </div>
                    {body}
                  </div>
                ))}
              </div>
            </MacWindow>
          }
        >
          <p>{c.meetings.p1}</p>
          <p className="text-[13px]">{c.meetings.p2}</p>
        </Row>
      </Section>
    </>
  );
}
